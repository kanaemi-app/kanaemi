//! Text dictionaries made into binary ones, which the IME opens without
//! reading every line.

use std::fs;
use std::path::{Path, PathBuf};

use kanaemi_config::binary_name;
use kanaemi_engine::{
    BinaryDictionary, ImeDictionaryError, ImeFormat, SkippedLine, SkkEncoding, SkkError,
    TextDictionary, convert_text, read_ime_dictionary, read_skk_dictionary, read_text_replacements,
    replace_file, text_digest, write_ime_dictionary, write_skk_dictionary,
};

use crate::cache::FileCache;

/// Whether the file is a binary dictionary; an unreadable one is not.
pub fn is_binary(path: &Path) -> bool {
    kanaemi_engine::is_binary(path).unwrap_or(false)
}

/// Converts the text dictionary `name` in `folder` and returns the binary
/// one's name. The new file replaces an old one whole, so an IME reading the
/// old one keeps reading it until it opens the dictionaries again.
pub fn convert(folder: &Path, name: &str) -> Result<String, String> {
    let source = folder.join(name);
    let text = fs::read(&source).map_err(|e| e.to_string())?;
    let (bytes, _) = convert_text(text);
    let target_name = binary_name(name);
    if target_name == name {
        return Err(format!("{name} は変換先と同じ名前です"));
    }
    replace_file(folder.join(&target_name), bytes).map_err(|e| e.to_string())?;
    Ok(target_name)
}

/// How a text dictionary stands against the binary one made from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conversion {
    /// No binary dictionary of its name.
    None,
    /// The binary dictionary was made from the text as it is now.
    Current,
    /// The text changed since, or the binary one does not say what it was
    /// made from.
    Stale,
}

/// Compares the text dictionary `name` with its binary one by the text's
/// SHA-256.
pub fn conversion(folder: &Path, name: &str) -> Conversion {
    thread_local! {
        static KNOWN: FileCache<Conversion> = FileCache::default();
    }
    let text = folder.join(name);
    let binary = folder.join(binary_name(name));
    if !binary.exists() {
        return Conversion::None;
    }
    KNOWN.with(|known| {
        known.get(&[text.clone(), binary.clone()], || {
            made_from(&text, &binary)
        })
    })
}

fn made_from(text: &Path, binary: &Path) -> Conversion {
    // A damaged binary dictionary counts as stale, so it can be made again.
    let made_from = BinaryDictionary::open(binary)
        .ok()
        .filter(|b| b.verify_checksums().is_ok())
        .and_then(|b| b.source_digest());
    let now = fs::read(text).ok().map(text_digest);
    match (made_from, now) {
        (Some(made_from), Some(now)) if made_from == now => Conversion::Current,
        _ => Conversion::Stale,
    }
}

/// The format of a dictionary another input method reads and writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DictionaryFormat {
    Ime(ImeFormat),
    Skk(SkkSource),
}

/// Where an SKK dictionary comes from: the user dictionary of an SKK
/// implementation, or any SKK dictionary file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkkSource {
    MacSkk,
    AquaSkk,
    IbusSkk,
    Fcitx5Skk,
    Fcitx5Cskk,
    CorvusSkk,
    Ddskk,
    Skkeleton,
    File,
}

impl SkkSource {
    pub const ALL: [Self; 9] = [
        Self::MacSkk,
        Self::AquaSkk,
        Self::IbusSkk,
        Self::Fcitx5Skk,
        Self::Fcitx5Cskk,
        Self::CorvusSkk,
        Self::Ddskk,
        Self::Skkeleton,
        Self::File,
    ];

    /// Whether it runs on the OS the settings app runs on.
    fn runs_here(self) -> bool {
        match self {
            Self::MacSkk | Self::AquaSkk => cfg!(target_os = "macos"),
            Self::IbusSkk | Self::Fcitx5Skk | Self::Fcitx5Cskk => {
                cfg!(all(unix, not(target_os = "macos")))
            }
            Self::CorvusSkk => cfg!(windows),
            Self::Ddskk | Self::Skkeleton | Self::File => true,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::MacSkk => "macSKK",
            Self::AquaSkk => "AquaSKK",
            Self::IbusSkk => "ibus-skk",
            Self::Fcitx5Skk => "fcitx5-skk",
            Self::Fcitx5Cskk => "fcitx5-cskk",
            Self::CorvusSkk => "CorvusSKK",
            Self::Ddskk => "DDSKK",
            Self::Skkeleton => "skkeleton",
            Self::File => "SKK 辞書",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::MacSkk => "macSKK のユーザー辞書を取り込みます",
            Self::AquaSkk => "AquaSKK のユーザー辞書を取り込みます",
            Self::IbusSkk => "ibus-skk のユーザー辞書を取り込みます",
            Self::Fcitx5Skk => "fcitx5-skk のユーザー辞書を取り込みます",
            Self::Fcitx5Cskk => "fcitx5-cskk のユーザー辞書を取り込みます",
            Self::CorvusSkk => "CorvusSKK のユーザー辞書を取り込みます",
            Self::Ddskk => "Emacs の DDSKK の個人辞書を取り込みます",
            Self::Skkeleton => "Vim・Neovim の skkeleton のユーザー辞書を取り込みます",
            Self::File => "任意の SKK 辞書ファイルを指定して取り込みます",
        }
    }

    /// Where its user dictionary is by default in the environment `env`, the
    /// first that exists standing: ddskk keeps it in `skk-user-directory`
    /// when that is set.
    fn user_dictionaries_in(self, env: &dyn Fn(&str) -> Option<PathBuf>) -> Vec<PathBuf> {
        let home = home_in(env);
        let under = |base: &Option<PathBuf>, path: &str| base.as_ref().map(|b| b.join(path));
        let config = env("XDG_CONFIG_HOME").or_else(|| under(&home, ".config"));
        let fcitx = env("FCITX_DATA_HOME")
            .or_else(|| env("XDG_DATA_HOME").map(|d| d.join("fcitx5")))
            .or_else(|| under(&home, ".local/share/fcitx5"));
        let paths = match self {
            Self::MacSkk => vec![under(
                &home,
                "Library/Containers/net.mtgto.inputmethod.macSKK/Data/Documents/Dictionaries/skk-jisyo.utf8",
            )],
            Self::AquaSkk => vec![under(
                &home,
                "Library/Application Support/AquaSKK/skk-jisyo.utf8",
            )],
            Self::IbusSkk => vec![under(&config, "ibus-skk/user.dict")],
            Self::Fcitx5Skk => vec![under(&fcitx, "skk/user.dict")],
            Self::Fcitx5Cskk => vec![under(&fcitx, "cskk/user.dict")],
            Self::CorvusSkk => vec![under(&env("APPDATA"), "CorvusSKK/userdict.txt")],
            Self::Ddskk => vec![under(&home, ".ddskk/jisyo"), under(&home, ".skk-jisyo")],
            Self::Skkeleton => vec![under(&home, ".skkeleton")],
            Self::File => Vec::new(),
        };
        paths.into_iter().flatten().collect()
    }

    /// The encoding it reads a dictionary in that does not say, and writes its
    /// user dictionary in without saying so; that of SKK-JISYO for any SKK
    /// dictionary. CorvusSKK writes UTF-16 with a byte order mark, which
    /// tells it, and reads UTF-8.
    pub fn encoding(self) -> SkkEncoding {
        match self {
            Self::MacSkk | Self::AquaSkk | Self::Skkeleton | Self::CorvusSkk => SkkEncoding::Utf8,
            Self::IbusSkk | Self::Fcitx5Skk | Self::Fcitx5Cskk | Self::Ddskk | Self::File => {
                SkkEncoding::EucJp
            }
        }
    }

    /// The folder it reads its dictionaries from, where one written out for
    /// it is best saved.
    fn dictionary_folder(self, env: &dyn Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
        match self {
            Self::MacSkk | Self::AquaSkk => self
                .user_dictionaries_in(env)
                .first()
                .and_then(|path| path.parent().map(Path::to_owned)),
            _ => None,
        }
    }
}

/// The environment the settings app runs in.
fn process_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn home_in(env: &dyn Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    env("HOME").or_else(|| env("USERPROFILE"))
}

impl DictionaryFormat {
    /// Every format, of input methods that run elsewhere too.
    pub fn all() -> Vec<Self> {
        let imes = ImeFormat::ALL.into_iter().map(Self::Ime);
        imes.chain(SkkSource::ALL.into_iter().map(Self::Skk))
            .collect()
    }

    /// The formats offered on this OS: that of the input method the OS comes
    /// with first (Mozc, which reads Google Japanese Input's, on Linux), then
    /// the SKK implementations that run here and any SKK dictionary, then
    /// the other input methods, whose exports are brought from elsewhere.
    pub fn offered() -> Vec<Self> {
        let native = Self::Ime(if cfg!(target_os = "macos") {
            ImeFormat::MacOs
        } else if cfg!(windows) {
            ImeFormat::MsIme
        } else {
            ImeFormat::Google
        });
        let skk = SkkSource::ALL
            .into_iter()
            .filter(|skk| skk.runs_here())
            .map(Self::Skk);
        let others = ImeFormat::ALL
            .into_iter()
            .map(Self::Ime)
            .filter(|f| *f != native);
        std::iter::once(native).chain(skk).chain(others).collect()
    }

    /// The name the settings app shows for it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ime(ImeFormat::MsIme) => "MS-IME",
            Self::Ime(ImeFormat::Google) => "Google 日本語入力",
            Self::Ime(ImeFormat::Atok) => "ATOK",
            Self::Ime(ImeFormat::MacOs) => "macOS",
            Self::Skk(skk) => skk.label(),
        }
    }

    /// What it is, for one who may know it by another name.
    pub fn description(self) -> &'static str {
        match self {
            Self::Ime(ImeFormat::MsIme) => "Windows に付いてくる Microsoft IME",
            Self::Ime(ImeFormat::Google) => "Google の日本語入力。Mozc も同じ形式です",
            Self::Ime(ImeFormat::Atok) => "ジャストシステムの日本語入力",
            Self::Ime(ImeFormat::MacOs) => "macOS に付いてくる日本語入力の「ユーザ辞書」",
            Self::Skk(skk) => skk.description(),
        }
    }

    /// The file name an export is offered under. macSKK takes a dictionary
    /// whose name holds `utf8` for UTF-8.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Ime(ImeFormat::MsIme) => "kanaemi-ms-ime.txt",
            Self::Ime(ImeFormat::Google) => "kanaemi-google.txt",
            Self::Ime(ImeFormat::Atok) => "kanaemi-atok.txt",
            Self::Ime(ImeFormat::MacOs) => "kanaemi.plist",
            Self::Skk(skk) => match skk.encoding() {
                SkkEncoding::Utf8 => "SKK-JISYO.kanaemi.utf8",
                SkkEncoding::EucJp => "SKK-JISYO.kanaemi",
            },
        }
    }

    /// The folder an export is offered in, where the input method reads
    /// dictionaries from when it has one.
    pub fn export_folder(self) -> Option<PathBuf> {
        match self {
            Self::Skk(skk) => skk.dictionary_folder(&process_env),
            Self::Ime(_) => None,
        }
    }
}

/// How an import of a format finds what it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    /// There is one place to read, so nothing is asked.
    Known(Source),
    /// The one place to read has nothing, as the message says.
    Missing(String),
    /// The user picks a file, starting in the folder it is likely in.
    Pick(Option<PathBuf>),
}

impl DictionaryFormat {
    /// Where an import in this format reads from, on this machine.
    pub fn finding(self) -> Finding {
        self.finding_in(&process_env)
    }

    /// Where an import in this format reads from, in the environment `env`.
    fn finding_in(self, env: &dyn Fn(&str) -> Option<PathBuf>) -> Finding {
        let home = home_in(env);
        match self {
            Self::Ime(ImeFormat::MacOs) if cfg!(target_os = "macos") => {
                Finding::Known(Source::TextReplacements)
            }
            // Where the SKK implementations here keep their dictionaries.
            Self::Skk(SkkSource::File) => Finding::Pick(
                SkkSource::ALL
                    .into_iter()
                    .filter(|skk| skk.runs_here())
                    .filter_map(|skk| skk.dictionary_folder(env))
                    .find(|folder| folder.is_dir())
                    .or(home),
            ),
            Self::Skk(skk) => {
                let paths = skk.user_dictionaries_in(env);
                match paths.iter().find(|path| path.is_file()) {
                    Some(path) => Finding::Known(Source::File {
                        path: path.clone(),
                        name: format!("{} のユーザー辞書", skk.label()),
                    }),
                    None => Finding::Missing(format!(
                        "{} のユーザー辞書が見つかりません（{}）",
                        skk.label(),
                        paths
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join("、")
                    )),
                }
            }
            // An export brought from another machine.
            Self::Ime(_) => Finding::Pick(
                home.as_ref()
                    .map(|home| home.join("Downloads"))
                    .filter(|d| d.is_dir())
                    .or(home),
            ),
        }
    }
}

/// Where an import reads from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A file, and the name the dictionary made from it is named after.
    File { path: PathBuf, name: String },
    /// The text replacements of macOS, its user dictionary, which it keeps
    /// in a database of its own rather than a file to pick.
    TextReplacements,
}

impl Source {
    /// A file the user picked, named after itself.
    pub fn picked(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self::File { path, name }
    }

    fn name(&self) -> &str {
        match self {
            Self::File { name, .. } => name,
            Self::TextReplacements => "macOS のユーザ辞書",
        }
    }
}

/// What an import will put in the dictionaries folder, read before anything
/// is written so it can be looked over.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportPreview {
    /// What the dictionary is named after.
    pub name: String,
    pub format: DictionaryFormat,
    /// The text dictionary lines it writes.
    pub text: String,
    pub skipped: Vec<SkippedLine>,
}

impl ImportPreview {
    /// The words it takes, as reading (an okurigana word's with its `*`),
    /// surface and conjugation type.
    pub fn words(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.text.lines().map(|line| {
            let mut fields = line.split('\t');
            let mut next = || fields.next().unwrap_or_default();
            (next(), next(), next())
        })
    }

    pub fn word_count(&self) -> usize {
        self.text.lines().count()
    }
}

/// Reads what importing `source` in `format` would take and leave out,
/// writing nothing.
pub fn preview_import(source: &Source, format: DictionaryFormat) -> Result<ImportPreview, String> {
    let (text, skipped) = match (source, format) {
        (Source::TextReplacements, _) => {
            let read = read_text_replacements(text_replacements()?);
            (read.text(), read.skipped)
        }
        (Source::File { path, .. }, DictionaryFormat::Ime(ime)) => {
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            let read = read_ime_dictionary(bytes, ime).map_err(ime_error)?;
            (read.text(), read.skipped)
        }
        (Source::File { path, .. }, DictionaryFormat::Skk(skk)) => {
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            let read = read_skk_dictionary(bytes, skk.encoding()).map_err(skk_error)?;
            (read.text, read.skipped)
        }
    };
    Ok(ImportPreview {
        name: source.name().to_owned(),
        format,
        text,
        skipped,
    })
}

/// Puts a previewed import into `folder` as a text dictionary named after
/// what it read, and says how it went. Its first line says where it came
/// from, which the settings app shows beside it.
pub fn write_import(preview: &ImportPreview, folder: &Path) -> Result<String, String> {
    if preview.word_count() == 0 {
        return Err("取り込める語がありません".to_owned());
    }
    // A file name may hold line breaks, which would start lines of their own
    // after the comment and give words the preview never showed.
    let source: String = preview
        .name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let text = format!(
        "# {source}（{}）から取り込んだ辞書\n{}",
        preview.format.label(),
        preview.text
    );
    // A file of that name, perhaps an import edited by hand, is kept: the
    // import takes the first free name.
    let name = (1..)
        .map(|n| match n {
            1 => format!("{source}.tsv"),
            n => format!("{source} {n}.tsv"),
        })
        // A binary dictionary of the name would stand for the new text one.
        .find(|name| !folder.join(name).exists() && !folder.join(binary_name(name)).exists())
        .expect("some name is free");
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    replace_file(folder.join(&name), text).map_err(|e| e.to_string())?;
    Ok(format!("{name} として取り込みました。"))
}

/// The (shortcut, phrase) pairs of the text replacements macOS keeps for
/// the user.
fn text_replacements() -> Result<Vec<(String, String)>, String> {
    let home = home_in(&process_env).ok_or("ホームフォルダが分かりません")?;
    let database = home.join("Library/KeyboardServices/TextReplacements.db");
    if !database.exists() {
        return Err("macOS のユーザ辞書が見つかりません".to_owned());
    }
    text_replacements_in(&database)
}

/// The (shortcut, phrase) pairs of a text replacements database, read with
/// the `sqlite3` macOS comes with. Deleted ones it keeps to sync are left
/// out.
fn text_replacements_in(database: &Path) -> Result<Vec<(String, String)>, String> {
    #[derive(serde::Deserialize)]
    struct Entry {
        shortcut: Option<String>,
        phrase: Option<String>,
    }
    let output = std::process::Command::new("/usr/bin/sqlite3")
        .arg("-readonly")
        .arg("-json")
        .arg(database)
        .arg(
            "SELECT ZSHORTCUT AS shortcut, ZPHRASE AS phrase FROM ZTEXTREPLACEMENTENTRY \
             WHERE ZWASDELETED IS NOT 1 ORDER BY Z_PK",
        )
        .output()
        .map_err(|e| format!("macOS のユーザ辞書を読めません（{e}）"))?;
    if !output.status.success() {
        return Err(format!(
            "macOS のユーザ辞書を読めません（{}）",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    // No rows print nothing at all, rather than an empty array.
    if output.stdout.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    let entries: Vec<Entry> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("macOS のユーザ辞書を読めません（{e}）"))?;
    Ok(entries
        .into_iter()
        .map(|e| (e.shortcut.unwrap_or_default(), e.phrase.unwrap_or_default()))
        .collect())
}

fn skk_error(error: SkkError) -> String {
    match error {
        SkkError::Undecodable { encoding, line } => format!(
            "{line} 行目を {encoding} として読めません。文字コードの指定（coding:）を確かめてください"
        ),
        SkkError::UnsupportedEncoding { name } => {
            format!("文字コード {name} には対応していません")
        }
    }
}

fn ime_error(error: ImeDictionaryError) -> String {
    match error {
        ImeDictionaryError::Undecodable { encoding, line } => {
            format!("{line} 行目を {encoding} として読めません")
        }
        ImeDictionaryError::NotPropertyList(why) => format!("plist として読めません（{why}）"),
        ImeDictionaryError::NotArray => "語の一覧の plist ではありません".to_owned(),
    }
}

/// Writes the user custom dictionary `custom` to `target` in `format`, and
/// says how it went.
pub fn export_dictionary(
    custom: &Path,
    target: &Path,
    format: DictionaryFormat,
) -> Result<String, String> {
    let (dictionary, _) = TextDictionary::read_user_custom(custom).map_err(|e| e.to_string())?;
    let export = match format {
        DictionaryFormat::Ime(ime) => write_ime_dictionary(&dictionary, ime),
        DictionaryFormat::Skk(skk) => write_skk_dictionary(&dictionary, skk.encoding()),
    };
    fs::write(target, &export.bytes).map_err(|e| e.to_string())?;
    let mut message = format!("{} 語を書き出しました。", export.written);
    if export.skipped > 0 {
        message.push_str(&format!(
            "{} 語は {} で表せないので書き出しませんでした。",
            export.skipped,
            format.label()
        ));
    }
    Ok(message)
}

/// Why the binary dictionary at `path` cannot be used, checking every byte.
pub fn verify(path: &Path) -> Result<(), String> {
    BinaryDictionary::open(path)
        .and_then(|binary| binary.verify_checksums())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use kanaemi_engine::{Dictionary, TextDictionary};

    use super::*;

    const GOOGLE: DictionaryFormat = DictionaryFormat::Ime(ImeFormat::Google);

    /// Previews the file and writes what it would take.
    fn import_dictionary(
        source: &Path,
        folder: &Path,
        format: DictionaryFormat,
    ) -> Result<String, String> {
        preview_import(&Source::picked(source.to_owned()), format)
            .and_then(|preview| write_import(&preview, folder))
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kanaemi-settings-convert-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        dir
    }

    #[test]
    fn converting_writes_a_binary_dictionary_beside_the_text() {
        let dir = temp_dir("convert");
        fs::write(dir.join("sub/a.tsv"), "# 説明\nきしゃ\t記者\n").unwrap();
        assert_eq!(convert(&dir, "sub/a.tsv"), Ok("sub/a.kdic".to_owned()));
        let path = dir.join("sub/a.kdic");
        assert!(is_binary(&path));
        assert!(!is_binary(&dir.join("sub/a.tsv")));
        assert_eq!(verify(&path), Ok(()));
        let binary = BinaryDictionary::open(&path).unwrap();
        assert_eq!(binary.lookup("きしゃ")[0].surface, "記者");
        assert_eq!(
            fs::read_dir(dir.join("sub")).unwrap().count(),
            2,
            "no partial file is left"
        );
    }

    #[test]
    fn converting_again_replaces_the_binary_dictionary() {
        let dir = temp_dir("again");
        fs::write(dir.join("a.tsv"), "きしゃ\t記者\n").unwrap();
        convert(&dir, "a.tsv").unwrap();
        fs::write(dir.join("a.tsv"), "きしゃ\t汽車\n").unwrap();
        convert(&dir, "a.tsv").unwrap();
        let binary = BinaryDictionary::open(dir.join("a.kdic")).unwrap();
        assert_eq!(binary.lookup("きしゃ")[0].surface, "汽車");
    }

    #[test]
    fn a_conversion_is_current_until_the_text_changes() {
        let dir = temp_dir("status");
        fs::write(dir.join("a.tsv"), "きしゃ\t記者\n").unwrap();
        assert_eq!(conversion(&dir, "a.tsv"), Conversion::None);
        convert(&dir, "a.tsv").unwrap();
        assert_eq!(conversion(&dir, "a.tsv"), Conversion::Current);
        fs::write(dir.join("a.tsv"), "きしゃ\t記者\nきしゃ\t汽車\n").unwrap();
        assert_eq!(conversion(&dir, "a.tsv"), Conversion::Stale);
        convert(&dir, "a.tsv").unwrap();
        assert_eq!(conversion(&dir, "a.tsv"), Conversion::Current);
    }

    #[test]
    fn a_damaged_binary_dictionary_is_stale_so_it_can_be_made_again() {
        let dir = temp_dir("damaged-status");
        fs::write(dir.join("a.tsv"), "きしゃ\t記者\n").unwrap();
        convert(&dir, "a.tsv").unwrap();
        let path = dir.join("a.kdic");
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, bytes).unwrap();

        assert_eq!(conversion(&dir, "a.tsv"), Conversion::Stale);
    }

    #[test]
    fn a_text_dictionary_named_like_a_binary_one_is_not_overwritten() {
        let dir = temp_dir("same-name");
        fs::write(dir.join("a.kdic"), "きしゃ\t記者\n").unwrap();
        assert!(convert(&dir, "a.kdic").is_err());
        assert_eq!(
            fs::read_to_string(dir.join("a.kdic")).unwrap(),
            "きしゃ\t記者\n"
        );
    }

    #[test]
    fn an_import_is_not_named_after_a_binary_dictionary_that_would_stand_for_it() {
        let dir = temp_dir("binary-left");
        let source = dir.join("google.txt");
        fs::write(&source, "きしゃ\t記者\t名詞\n").unwrap();
        let folder = dir.join("dictionaries");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(binary_name("google.txt.tsv")), b"old").unwrap();
        assert_eq!(
            import_dictionary(&source, &folder, GOOGLE),
            Ok("google.txt 2.tsv として取り込みました。".to_owned())
        );
    }

    #[test]
    fn a_name_with_line_breaks_stays_in_the_comment_line() {
        let dir = temp_dir("name-breaks");
        let path = dir.join("google.txt");
        fs::write(&path, "きしゃ\t記者\t名詞\n").unwrap();
        let source = Source::File {
            path,
            name: "x\nねこ\t犬\n#".to_owned(),
        };
        let preview = preview_import(&source, GOOGLE).unwrap();
        let folder = dir.join("dictionaries");
        let written = write_import(&preview, &folder).unwrap();
        let name = written.strip_suffix(" として取り込みました。").unwrap();
        let text = fs::read_to_string(folder.join(name)).unwrap();
        let (dictionary, _) = TextDictionary::parse(&text);
        assert!(dictionary.lookup("ねこ").is_empty(), "{text}");
        assert_eq!(text.lines().count(), 2, "{text}");
    }

    #[test]
    fn another_imes_words_are_imported_as_a_dictionary_of_their_own() {
        let dir = temp_dir("ime-import");
        let source = dir.join("google.txt");
        fs::write(
            &source,
            "# Google\nきしゃ\t記者\t名詞\nかく\t書く\t動詞カ行五段\nきしゃ\t記者\t名詞\nabc\tABC\t名詞\n単\nねこ\t猫\t抑制単語\n",
        )
        .unwrap();
        let folder = dir.join("dictionaries");
        let preview = preview_import(&Source::picked(source), GOOGLE).unwrap();
        assert_eq!(
            preview.words().collect::<Vec<_>>(),
            [("きしゃ", "記者", ""), ("か", "書", "五段-カ行")]
        );
        let skipped: Vec<(usize, &str)> = preview
            .skipped
            .iter()
            .map(|s| (s.line, s.text.as_str()))
            .collect();
        assert_eq!(
            skipped,
            [(5, "abc\tABC\t名詞"), (6, "単"), (7, "ねこ\t猫\t抑制単語")]
        );
        assert!(!folder.exists(), "nothing is written until it is confirmed");
        assert_eq!(
            write_import(&preview, &folder),
            Ok("google.txt.tsv として取り込みました。".to_owned())
        );
        assert_eq!(
            fs::read_to_string(folder.join("google.txt.tsv")).unwrap(),
            "# google.txt（Google 日本語入力）から取り込んだ辞書\nきしゃ\t記者\nか\t書\t五段-カ行\n"
        );
    }

    /// An environment with only `vars`.
    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<PathBuf> + use<> {
        let vars: Vec<(String, PathBuf)> = vars
            .iter()
            .map(|(name, value)| ((*name).to_owned(), PathBuf::from(value)))
            .collect();
        move |name| vars.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }

    #[test]
    fn each_skk_implementation_looks_for_its_user_dictionary_where_it_keeps_it() {
        let env = env(&[("HOME", "/h")]);
        let paths = |skk: SkkSource| skk.user_dictionaries_in(&env);
        let at = |paths: &[&str]| paths.iter().map(PathBuf::from).collect::<Vec<_>>();
        assert_eq!(
            paths(SkkSource::MacSkk),
            at(&[
                "/h/Library/Containers/net.mtgto.inputmethod.macSKK/Data/Documents/Dictionaries/skk-jisyo.utf8"
            ])
        );
        assert_eq!(
            paths(SkkSource::AquaSkk),
            at(&["/h/Library/Application Support/AquaSKK/skk-jisyo.utf8"])
        );
        assert_eq!(
            paths(SkkSource::IbusSkk),
            at(&["/h/.config/ibus-skk/user.dict"])
        );
        assert_eq!(
            paths(SkkSource::Fcitx5Skk),
            at(&["/h/.local/share/fcitx5/skk/user.dict"])
        );
        assert_eq!(
            paths(SkkSource::Fcitx5Cskk),
            at(&["/h/.local/share/fcitx5/cskk/user.dict"])
        );
        assert_eq!(
            paths(SkkSource::Ddskk),
            at(&["/h/.ddskk/jisyo", "/h/.skk-jisyo"]),
            "skk-user-directory first"
        );
        assert_eq!(paths(SkkSource::Skkeleton), at(&["/h/.skkeleton"]));
        assert_eq!(paths(SkkSource::CorvusSkk), at(&[]), "no APPDATA");
        assert_eq!(paths(SkkSource::File), at(&[]));
    }

    #[test]
    fn the_folders_skk_implementations_use_follow_the_environment() {
        let at = |skk: SkkSource, vars: &[(&str, &str)]| skk.user_dictionaries_in(&env(vars));
        let xdg = [
            ("HOME", "/h"),
            ("XDG_CONFIG_HOME", "/c"),
            ("XDG_DATA_HOME", "/d"),
        ];
        assert_eq!(
            at(SkkSource::IbusSkk, &xdg),
            [PathBuf::from("/c/ibus-skk/user.dict")]
        );
        assert_eq!(
            at(SkkSource::Fcitx5Skk, &xdg),
            [PathBuf::from("/d/fcitx5/skk/user.dict")]
        );
        assert_eq!(
            at(
                SkkSource::Fcitx5Cskk,
                &[("XDG_DATA_HOME", "/d"), ("FCITX_DATA_HOME", "/f")]
            ),
            [PathBuf::from("/f/cskk/user.dict")]
        );
        assert_eq!(
            at(SkkSource::CorvusSkk, &[("APPDATA", "/a")]),
            [PathBuf::from("/a/CorvusSKK/userdict.txt")]
        );
        assert_eq!(
            at(SkkSource::Skkeleton, &[("USERPROFILE", "/u")]),
            [PathBuf::from("/u/.skkeleton")],
            "the home on Windows"
        );
    }

    #[test]
    fn each_skk_implementation_is_read_in_the_encoding_it_writes() {
        use SkkEncoding::{EucJp, Utf8};
        for (skk, encoding) in [
            (SkkSource::MacSkk, Utf8),
            (SkkSource::AquaSkk, Utf8),
            (SkkSource::Skkeleton, Utf8),
            (SkkSource::CorvusSkk, Utf8),
            (SkkSource::IbusSkk, EucJp),
            (SkkSource::Fcitx5Skk, EucJp),
            (SkkSource::Fcitx5Cskk, EucJp),
            (SkkSource::Ddskk, EucJp),
            (SkkSource::File, EucJp),
        ] {
            assert_eq!(skk.encoding(), encoding, "{skk:?}");
        }
    }

    #[test]
    fn an_skk_user_dictionary_is_read_where_it_is_or_reported_missing() {
        let dir = temp_dir("finding");
        let home = dir.to_str().unwrap();
        let env = env(&[("HOME", home)]);
        let skkeleton = DictionaryFormat::Skk(SkkSource::Skkeleton);
        let Finding::Missing(message) = skkeleton.finding_in(&env) else {
            panic!("not missing");
        };
        assert!(
            message.contains(&dir.join(".skkeleton").display().to_string()),
            "{message}"
        );
        fs::write(dir.join(".skkeleton"), "").unwrap();
        assert_eq!(
            skkeleton.finding_in(&env),
            Finding::Known(Source::File {
                path: dir.join(".skkeleton"),
                name: "skkeleton のユーザー辞書".to_owned(),
            })
        );
        fs::write(dir.join(".skk-jisyo"), "").unwrap();
        assert!(matches!(
            DictionaryFormat::Skk(SkkSource::Ddskk).finding_in(&env),
            Finding::Known(Source::File { path, .. }) if path == dir.join(".skk-jisyo")
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn text_replacements_are_read_from_their_database_without_the_deleted() {
        let dir = temp_dir("text-replacements");
        let database = dir.join("TextReplacements.db");
        let made = std::process::Command::new("/usr/bin/sqlite3")
            .arg(&database)
            .arg(
                "CREATE TABLE ZTEXTREPLACEMENTENTRY (Z_PK INTEGER PRIMARY KEY, \
                 ZWASDELETED INTEGER, ZSHORTCUT VARCHAR, ZPHRASE VARCHAR);",
            )
            .status()
            .unwrap();
        assert!(made.success());
        assert_eq!(text_replacements_in(&database), Ok(Vec::new()), "no rows");
        let filled = std::process::Command::new("/usr/bin/sqlite3")
            .arg(&database)
            .arg(
                "INSERT INTO ZTEXTREPLACEMENTENTRY VALUES \
                 (1, 0, 'きしゃ', '記者'), (2, 1, 'ねこ', '猫'), (3, NULL, 'omw', 'On my way!');",
            )
            .status()
            .unwrap();
        assert!(filled.success());
        assert_eq!(
            text_replacements_in(&database),
            Ok(vec![
                ("きしゃ".to_owned(), "記者".to_owned()),
                ("omw".to_owned(), "On my way!".to_owned()),
            ])
        );
    }

    #[test]
    fn every_format_is_offered_at_most_once() {
        let offered = DictionaryFormat::offered();
        let all = DictionaryFormat::all();
        for format in &offered {
            assert!(all.contains(format), "{format:?}");
            assert_eq!(
                offered.iter().filter(|f| *f == format).count(),
                1,
                "{format:?}"
            );
        }
        assert!(offered.contains(&DictionaryFormat::Skk(SkkSource::File)));
        assert!(offered.contains(&DictionaryFormat::Skk(SkkSource::Ddskk)));
    }

    #[test]
    fn an_skk_user_dictionary_in_utf_8_without_coding_is_read_as_its_writer_wrote_it() {
        let dir = temp_dir("aquaskk");
        let path = dir.join("skk-jisyo.utf8");
        fs::write(&path, ";; okuri-nasi entries.\nきしゃ /記者/\n").unwrap();
        let source = Source::File {
            path,
            name: "AquaSKK のユーザー辞書".to_owned(),
        };
        let preview = preview_import(&source, DictionaryFormat::Skk(SkkSource::AquaSkk)).unwrap();
        assert_eq!(preview.words().next(), Some(("きしゃ", "記者", "")));
    }

    #[test]
    fn a_file_without_a_word_to_take_is_not_imported() {
        let dir = temp_dir("ime-empty");
        let source = dir.join("empty.txt");
        fs::write(&source, "# Google\nabc\tABC\t名詞\n").unwrap();
        let folder = dir.join("dictionaries");
        assert_eq!(
            import_dictionary(&source, &folder, GOOGLE),
            Err("取り込める語がありません".to_owned())
        );
        assert!(!folder.join("empty.txt.tsv").exists());
    }

    #[test]
    fn a_file_another_ime_cannot_have_written_is_not_imported() {
        let dir = temp_dir("ime-broken");
        let source = dir.join("broken.txt");
        fs::write(&source, b"\x82\xa0\t\x82").unwrap();
        let folder = dir.join("dictionaries");
        assert_eq!(
            import_dictionary(&source, &folder, DictionaryFormat::Ime(ImeFormat::MsIme)),
            Err("1 行目を Shift_JIS として読めません".to_owned())
        );
        assert!(!folder.join("broken.txt.tsv").exists());
    }

    #[test]
    fn the_user_custom_dictionary_is_exported_for_another_ime() {
        let dir = temp_dir("ime-export");
        let custom = dir.join("custom.tsv");
        fs::write(&custom, "きしゃ\t記者\n{}こ\t{}個\n").unwrap();
        let target = dir.join("out.txt");
        assert_eq!(
            export_dictionary(&custom, &target, GOOGLE),
            Ok("1 語を書き出しました。1 語は Google 日本語入力 で表せないので書き出しませんでした。".to_owned())
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "きしゃ\t記者\t名詞\n");
    }

    #[test]
    fn the_user_custom_dictionary_is_exported_as_an_skk_dictionary() {
        let dir = temp_dir("skk-export");
        let custom = dir.join("custom.tsv");
        fs::write(&custom, "きしゃ\t記者\nか\t書\t五段-カ行\n").unwrap();
        let target = dir.join("SKK-JISYO.out");
        assert_eq!(
            export_dictionary(&custom, &target, DictionaryFormat::Skk(SkkSource::File)),
            Ok("2 語を書き出しました。".to_owned())
        );
        let folder = dir.join("dictionaries");
        import_dictionary(&target, &folder, DictionaryFormat::Skk(SkkSource::File)).unwrap();
        let text = fs::read_to_string(folder.join("SKK-JISYO.out.tsv")).unwrap();
        let (dictionary, invalid) = TextDictionary::parse(text);
        assert_eq!(invalid, []);
        assert_eq!(dictionary.lookup("きしゃ")[0].surface, "記者");
    }

    #[test]
    fn an_skk_dictionary_is_imported_as_text_under_its_name() {
        let dir = temp_dir("skk");
        let source = dir.join("SKK-JISYO.test");
        fs::write(&source, ";; -*- coding: utf-8 -*-\nきしゃ /記者/汽車/\n").unwrap();
        let folder = dir.join("dictionaries");
        assert_eq!(
            import_dictionary(&source, &folder, DictionaryFormat::Skk(SkkSource::File)),
            Ok("SKK-JISYO.test.tsv として取り込みました。".to_owned())
        );
        let text = fs::read_to_string(folder.join("SKK-JISYO.test.tsv")).unwrap();
        assert!(text.starts_with("# SKK-JISYO.test（SKK 辞書）から取り込んだ辞書\n"));
        let (dictionary, invalid) = TextDictionary::parse(text);
        assert_eq!(invalid, []);
        assert_eq!(dictionary.lookup("きしゃ")[0].surface, "記者");
    }

    #[test]
    fn an_skk_dictionary_its_encoding_cannot_read_is_not_imported() {
        let dir = temp_dir("skk-undecodable");
        let source = dir.join("SKK-JISYO.broken");
        fs::write(&source, b";; -*- coding: euc-jp -*-\n\xa4\xaf /\xae\xa1/\n").unwrap();
        let folder = dir.join("dictionaries");

        let imported = import_dictionary(&source, &folder, DictionaryFormat::Skk(SkkSource::File));

        assert_eq!(
            imported,
            Err(
                "2 行目を EUC-JP として読めません。文字コードの指定（coding:）を確かめてください"
                    .to_owned()
            )
        );
        assert!(!folder.join("SKK-JISYO.broken.tsv").exists());
    }

    #[test]
    fn an_skk_dictionary_in_a_jis_x_0213_encoding_is_not_imported() {
        let dir = temp_dir("skk-unsupported");
        let source = dir.join("SKK-JISYO.2004");
        fs::write(
            &source,
            b";; -*- coding: shift_jis-2004 -*-\n\x82\xa9 /\xed\x40/\n",
        )
        .unwrap();
        let folder = dir.join("dictionaries");

        let imported = import_dictionary(&source, &folder, DictionaryFormat::Skk(SkkSource::File));

        assert_eq!(
            imported,
            Err("文字コード shift_jis-2004 には対応していません".to_owned())
        );
        assert!(!folder.join("SKK-JISYO.2004.tsv").exists());
    }

    #[test]
    fn importing_again_keeps_the_earlier_import() {
        let dir = temp_dir("skk-again");
        let source = dir.join("SKK-JISYO.test");
        fs::write(&source, ";; -*- coding: utf-8 -*-\nきしゃ /記者/\n").unwrap();
        let folder = dir.join("dictionaries");
        import_dictionary(&source, &folder, DictionaryFormat::Skk(SkkSource::File)).unwrap();
        fs::write(folder.join("SKK-JISYO.test.tsv"), "edited\n").unwrap();
        assert_eq!(
            import_dictionary(&source, &folder, DictionaryFormat::Skk(SkkSource::File)),
            Ok("SKK-JISYO.test 2.tsv として取り込みました。".to_owned())
        );
        assert_eq!(
            fs::read_to_string(folder.join("SKK-JISYO.test.tsv")).unwrap(),
            "edited\n"
        );
    }

    #[test]
    fn a_missing_file_is_not_converted() {
        let dir = temp_dir("missing");
        assert!(convert(&dir, "none.tsv").is_err());
        assert!(!dir.join("none.kdic").exists());
    }

    #[test]
    fn a_damaged_binary_dictionary_fails_verification() {
        let dir = temp_dir("damaged");
        fs::write(dir.join("a.tsv"), "きしゃ\t記者\n").unwrap();
        convert(&dir, "a.tsv").unwrap();
        let path = dir.join("a.kdic");
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(verify(&path).is_err());
    }
}
