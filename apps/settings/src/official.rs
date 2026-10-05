//! The official dictionaries kanaemi-dict ships, fetched and installed when
//! the user asks: the catalog says which there are and what each holds.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use kanaemi_config::{DICTIONARY_DIR, MODEL_FILE, binary_name};
use kanaemi_engine::{MODEL_FORMAT_VERSION, replace_file, text_digest};
use serde::Deserialize;

use crate::cache::FileCache;
use crate::convert::convert;

/// Where the latest release's files are.
const RELEASE: &str = "https://github.com/kanaemi-app/kanaemi-dict/releases/latest/download/";
const CATALOG: &str = "index.json";
/// The version of the catalog's shape this app reads.
const CATALOG_FORMAT: u32 = 1;
/// The folder of the official dictionaries in the dictionaries folder, each
/// in a folder of its name.
pub const FOLDER: &str = "kanaemi";
/// Far beyond any dictionary, to stop an endless download.
const MAX_DOWNLOAD: u64 = 1 << 30;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
/// Long enough for a base dictionary over a slow line.
const BODY_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const NOTICE: &str = "NOTICE";
const LICENSE: &str = "LICENSE";

/// The dictionaries of a release.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Catalog {
    pub dictionaries: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Entry {
    pub name: String,
    pub base: bool,
    pub label: String,
    /// The file of the release holding the dictionary's folder.
    pub archive: String,
    pub dictionary: Content,
    /// The ranking model paired with a base dictionary.
    pub model: Option<Model>,
}

/// A dictionary file in an archive.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Content {
    pub file: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Model {
    pub format: u32,
    pub sha256: String,
}

/// How an official dictionary stands in the settings folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Missing,
    Current,
    /// Installed, but the release holds another version.
    Outdated,
}

impl Catalog {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Format {
            format: u32,
        }
        let unreadable = |e: serde_json::Error| format!("目録を読めません（{e}）");
        let Format { format } = serde_json::from_slice(bytes).map_err(unreadable)?;
        if format != CATALOG_FORMAT {
            return Err(format!(
                "目録の形の版 {format} は読めません。かなえみを新しくしてください"
            ));
        }
        let catalog: Catalog = serde_json::from_slice(bytes).map_err(unreadable)?;
        // The names become paths in the settings folder and the release.
        for entry in &catalog.dictionaries {
            for name in [&entry.name, &entry.archive, &entry.dictionary.file] {
                if !is_plain_name(name) {
                    return Err(format!("目録の名前 {name:?} は使えません"));
                }
            }
        }
        Ok(catalog)
    }
}

/// A file name of its own, which stays in the folder it is joined to.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':'])
}

impl Entry {
    /// The dictionary's name in the dictionary list once installed.
    pub fn listed_name(&self) -> String {
        binary_name(self.text_name())
    }

    /// Its text dictionary's name in the dictionary list.
    fn text_name(&self) -> String {
        format!("{FOLDER}/{}/{}", self.name, self.dictionary.file)
    }

    pub fn archive_url(&self) -> String {
        format!("{RELEASE}{}", self.archive)
    }

    fn folder(&self, dir: &Path) -> PathBuf {
        dir.join(DICTIONARY_DIR).join(FOLDER).join(&self.name)
    }

    /// Why this app cannot install it, if it cannot.
    pub fn refusal(&self) -> Option<String> {
        let model = self.model.as_ref()?;
        (model.format != MODEL_FORMAT_VERSION).then(|| {
            format!(
                "並べ替えのモデルの形式の版 {} は読めません。かなえみを新しくしてください",
                model.format
            )
        })
    }

    pub fn status(&self, dir: &Path) -> Status {
        let text = self.folder(dir).join(&self.dictionary.file);
        if !text.exists() {
            return Status::Missing;
        }
        let current = sha256_of(&text).as_deref() == Some(self.dictionary.sha256.as_str())
            && self.model.as_ref().is_none_or(|model| {
                sha256_of(&dir.join(MODEL_FILE)).as_deref() == Some(model.sha256.as_str())
            });
        if current {
            Status::Current
        } else {
            Status::Outdated
        }
    }

    /// Puts what `archive` holds for this dictionary into the settings folder
    /// `dir`, once it matches the catalog. Nothing changes when it does not.
    pub fn install(&self, dir: &Path, archive: &[u8]) -> Result<(), String> {
        if let Some(refusal) = self.refusal() {
            return Err(refusal);
        }
        let files = self.unpack(archive)?;
        // Made whole beside the dictionaries folder, where the IME does not
        // look, then moved in file by file.
        let staged = dir.join(format!(".{FOLDER}-{}.partial", self.name));
        let _ = fs::remove_dir_all(&staged);
        let result = (|| {
            fs::create_dir_all(&staged).map_err(|e| e.to_string())?;
            for (name, bytes) in &files.folder {
                fs::write(staged.join(name), bytes).map_err(|e| e.to_string())?;
            }
            convert(&staged, &self.dictionary.file)?;
            let folder = self.folder(dir);
            fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            // The text dictionary goes last: its digest is what `status`
            // reads, so a move that fails halfway still shows as outdated.
            let mut names: Vec<_> = fs::read_dir(&staged)
                .map_err(|e| e.to_string())?
                .map(|entry| entry.map(|e| e.file_name()))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?;
            names.sort_by_key(|name| name == self.dictionary.file.as_str());
            for name in names {
                fs::rename(staged.join(&name), folder.join(&name)).map_err(|e| e.to_string())?;
            }
            if let Some(model) = &files.model {
                replace_file(dir.join(MODEL_FILE), model).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        let _ = fs::remove_dir_all(&staged);
        result
    }

    /// The files of this dictionary in `archive`, each checked against the
    /// catalog.
    fn unpack(&self, archive: &[u8]) -> Result<Unpacked, String> {
        let unreadable = |e: zip::result::ZipError| format!("配布物を読めません（{e}）");
        let mut zip = zip::ZipArchive::new(Cursor::new(archive)).map_err(unreadable)?;
        let mut read = |name: &str| -> Result<Option<Vec<u8>>, String> {
            let mut file = match zip.by_name(&format!("{}/{name}", self.name)) {
                Ok(file) => file,
                Err(zip::result::ZipError::FileNotFound) => return Ok(None),
                Err(e) => return Err(unreadable(e)),
            };
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|e| format!("配布物を読めません（{e}）"))?;
            Ok(Some(bytes))
        };
        let missing = |name: &str| format!("配布物に {name} がありません");
        let mismatch = |name: &str| {
            format!("配布物の {name} が目録と合いません。もう一度確かめてから入れてください")
        };
        let text = read(&self.dictionary.file)?.ok_or_else(|| missing(&self.dictionary.file))?;
        if hex(&text_digest(&text)) != self.dictionary.sha256 {
            return Err(mismatch(&self.dictionary.file));
        }
        let mut folder = vec![(self.dictionary.file.clone(), text)];
        for name in [NOTICE, LICENSE] {
            folder.push((name.to_owned(), read(name)?.ok_or_else(|| missing(name))?));
        }
        let model = match &self.model {
            None => None,
            Some(model) => {
                let bytes = read(MODEL_FILE)?.ok_or_else(|| missing(MODEL_FILE))?;
                if hex(&text_digest(&bytes)) != model.sha256 {
                    return Err(mismatch(MODEL_FILE));
                }
                Some(bytes)
            }
        };
        Ok(Unpacked { folder, model })
    }
}

struct Unpacked {
    /// The files that go into the dictionary's folder, by name.
    folder: Vec<(String, Vec<u8>)>,
    model: Option<Vec<u8>>,
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A file's SHA-256, worked out again only when it changed: a base
/// dictionary is large.
fn sha256_of(path: &Path) -> Option<String> {
    thread_local! {
        static KNOWN: FileCache<Option<String>> = FileCache::default();
    }
    KNOWN.with(|known| {
        known.get(&[path.to_owned()], || {
            fs::read(path).ok().map(|bytes| hex(&text_digest(bytes)))
        })
    })
}

/// The catalog of the latest release.
pub fn fetch_catalog() -> Result<Catalog, String> {
    Catalog::parse(&fetch(&format!("{RELEASE}{CATALOG}"))?)
}

/// The bytes at `url`.
pub fn fetch(url: &str) -> Result<Vec<u8>, String> {
    // ureq waits forever by default, which would leave the buttons disabled
    // until the app is restarted when a connection stalls.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(RESPONSE_TIMEOUT))
        .timeout_recv_body(Some(BODY_TIMEOUT))
        .build()
        .into();
    agent
        .get(url)
        .call()
        .and_then(|mut response| {
            response
                .body_mut()
                .with_config()
                .limit(MAX_DOWNLOAD)
                .read_to_vec()
        })
        .map_err(|e| format!("取れません（{e}）"))
}

/// The dictionary list with `entry` in it: in place of its text dictionary,
/// or a base one before the first official dictionary, an additional one
/// last.
pub fn placed(mut list: Vec<String>, entry: &Entry) -> Vec<String> {
    let name = entry.listed_name();
    if list.contains(&name) {
        return list;
    }
    if let Some(text) = list.iter_mut().find(|n| **n == entry.text_name()) {
        *text = name;
        return list;
    }
    let official = format!("{FOLDER}/");
    let at = match entry.base {
        true => list
            .iter()
            .position(|n| n.starts_with(&official))
            .unwrap_or(list.len()),
        false => list.len(),
    };
    list.insert(at, name);
    list
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use kanaemi_engine::{BinaryDictionary, Dictionary};

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kanaemi-settings-official-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const BASE: &str = "# Kanaemi 公式辞書・基本\nきしゃ\t記者\t\t20\n";
    const IT: &str = "# Kanaemi 公式辞書・IT\nくらうど\tクラウド\t\t10\n";

    fn model_bytes() -> Vec<u8> {
        let body = vec![0u8; 1 << 10];
        let mut file = Vec::new();
        file.extend_from_slice(b"KANAEMIM");
        file.extend_from_slice(&MODEL_FORMAT_VERSION.to_le_bytes());
        file.extend_from_slice(&[10, 1, 0, 0]);
        file.extend_from_slice(&1.0f32.to_le_bytes());
        file.extend_from_slice(&[0; 4]);
        file.extend_from_slice(&xxhash_rust::xxh3::xxh3_64(&body).to_le_bytes());
        file.extend_from_slice(&body);
        file
    }

    fn entry(name: &str, text: &str, model: Option<&[u8]>) -> Entry {
        Entry {
            name: name.to_owned(),
            base: model.is_some(),
            label: format!("{name} の説明"),
            archive: format!("kanaemi-{name}.zip"),
            dictionary: Content {
                file: format!("kanaemi-{name}.tsv"),
                size: text.len() as u64,
                sha256: hex(&text_digest(text)),
            },
            model: model.map(|m| Model {
                format: MODEL_FORMAT_VERSION,
                sha256: hex(&text_digest(m)),
            }),
        }
    }

    /// An archive as the release makes it: the files in a folder of the name.
    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (path, bytes) in files {
            zip.start_file(*path, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn base_archive(text: &str, model: &[u8]) -> Vec<u8> {
        archive(&[
            ("base/kanaemi-base.tsv", text.as_bytes()),
            ("base/NOTICE", b"notice"),
            ("base/LICENSE", b"CC BY 4.0"),
            ("base/ranking.model", model),
        ])
    }

    #[test]
    fn a_catalog_of_the_known_format_is_read() {
        let json = r#"{
            "format": 1,
            "dictionaries": [{
                "name": "base", "base": true, "label": "基本", "archive": "kanaemi-base.zip",
                "dictionary": { "file": "kanaemi-base.tsv", "size": 10, "sha256": "ab" },
                "model": { "format": 3, "sha256": "cd" }
            }, {
                "name": "it", "base": false, "label": "IT", "archive": "kanaemi-it.zip",
                "dictionary": { "file": "kanaemi-it.tsv", "size": 5, "sha256": "ef" }
            }]
        }"#;
        let catalog = Catalog::parse(json.as_bytes()).unwrap();
        assert_eq!(catalog.dictionaries.len(), 2);
        assert_eq!(catalog.dictionaries[0].model.as_ref().unwrap().format, 3);
        assert_eq!(catalog.dictionaries[1].model, None);
    }

    #[test]
    fn a_catalog_of_another_format_is_refused_by_its_version() {
        let error = Catalog::parse(br#"{ "format": 2, "anything": [] }"#).unwrap_err();
        assert!(error.contains('2'), "{error}");
        assert!(Catalog::parse(b"not json").is_err());
    }

    #[test]
    fn a_catalog_naming_a_file_outside_its_folder_is_refused() {
        for (name, file, archive) in [
            ("..", "kanaemi-base.tsv", "kanaemi-base.zip"),
            ("base", "../custom.tsv", "kanaemi-base.zip"),
            ("base", "kanaemi-base.tsv", "../index.json"),
            ("a/b", "kanaemi-base.tsv", "kanaemi-base.zip"),
            ("", "kanaemi-base.tsv", "kanaemi-base.zip"),
        ] {
            let json = format!(
                r#"{{ "format": 1, "dictionaries": [{{
                    "name": "{name}", "base": false, "label": "", "archive": "{archive}",
                    "dictionary": {{ "file": "{file}", "size": 0, "sha256": "" }}
                }}] }}"#
            );
            assert!(
                Catalog::parse(json.as_bytes()).is_err(),
                "{name} {file} {archive}"
            );
        }
    }

    #[test]
    fn an_installed_dictionary_is_named_after_its_binary_one() {
        assert_eq!(
            entry("it", IT, None).listed_name(),
            "kanaemi/it/kanaemi-it.kdic"
        );
    }

    #[test]
    fn a_base_dictionary_with_a_model_this_app_cannot_read_is_refused() {
        let model = model_bytes();
        let mut base = entry("base", BASE, Some(&model));
        assert_eq!(base.refusal(), None);
        base.model.as_mut().unwrap().format = MODEL_FORMAT_VERSION + 1;
        assert!(base.refusal().is_some());
        let dir = temp_dir("refused");
        assert!(base.install(&dir, &base_archive(BASE, &model)).is_err());
        assert!(!dir.join(DICTIONARY_DIR).exists());
        assert!(!dir.join(MODEL_FILE).exists());
    }

    #[test]
    fn installing_puts_the_dictionary_its_binary_and_the_model_in_place() {
        let dir = temp_dir("install");
        let model = model_bytes();
        let base = entry("base", BASE, Some(&model));
        assert_eq!(base.status(&dir), Status::Missing);

        base.install(&dir, &base_archive(BASE, &model)).unwrap();

        let folder = dir.join(DICTIONARY_DIR).join("kanaemi/base");
        assert_eq!(
            fs::read_to_string(folder.join("kanaemi-base.tsv")).unwrap(),
            BASE
        );
        assert_eq!(fs::read_to_string(folder.join("NOTICE")).unwrap(), "notice");
        assert_eq!(
            fs::read_to_string(folder.join("LICENSE")).unwrap(),
            "CC BY 4.0"
        );
        let binary = BinaryDictionary::open(folder.join("kanaemi-base.kdic")).unwrap();
        assert_eq!(binary.lookup("きしゃ")[0].surface, "記者");
        assert_eq!(fs::read(dir.join(MODEL_FILE)).unwrap(), model);
        assert_eq!(base.status(&dir), Status::Current);
        let mut left: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [DICTIONARY_DIR, MODEL_FILE],
            "nothing partial is left"
        );
    }

    #[test]
    fn an_additional_dictionary_leaves_the_model_alone() {
        let dir = temp_dir("additional");
        fs::write(dir.join(MODEL_FILE), "mine").unwrap();
        let it = entry("it", IT, None);
        it.install(
            &dir,
            &archive(&[
                ("it/kanaemi-it.tsv", IT.as_bytes()),
                ("it/NOTICE", b"notice"),
                ("it/LICENSE", b"CC BY 4.0"),
            ]),
        )
        .unwrap();
        assert_eq!(it.status(&dir), Status::Current);
        assert_eq!(fs::read_to_string(dir.join(MODEL_FILE)).unwrap(), "mine");
    }

    #[test]
    fn a_newer_release_shows_as_outdated_and_installing_it_replaces_the_old() {
        let dir = temp_dir("update");
        let model = model_bytes();
        entry("base", BASE, Some(&model))
            .install(&dir, &base_archive(BASE, &model))
            .unwrap();
        let newer_text = "# Kanaemi 公式辞書・基本\nきしゃ\t汽車\t\t20\n";
        let newer = entry("base", newer_text, Some(&model));
        assert_eq!(newer.status(&dir), Status::Outdated);

        newer
            .install(&dir, &base_archive(newer_text, &model))
            .unwrap();

        assert_eq!(newer.status(&dir), Status::Current);
        let folder = dir.join(DICTIONARY_DIR).join("kanaemi/base");
        let binary = BinaryDictionary::open(folder.join("kanaemi-base.kdic")).unwrap();
        assert_eq!(binary.lookup("きしゃ")[0].surface, "汽車");
    }

    #[test]
    fn a_base_dictionary_whose_model_changed_is_outdated() {
        let dir = temp_dir("model-changed");
        let model = model_bytes();
        let base = entry("base", BASE, Some(&model));
        base.install(&dir, &base_archive(BASE, &model)).unwrap();
        fs::write(dir.join(MODEL_FILE), "another model").unwrap();
        assert_eq!(base.status(&dir), Status::Outdated);
    }

    #[test]
    fn an_archive_that_does_not_match_the_catalog_changes_nothing() {
        let dir = temp_dir("mismatch");
        let model = model_bytes();
        let base = entry("base", BASE, Some(&model));
        for archive in [
            base_archive("# 別の中身\n", &model),
            base_archive(BASE, b"another model"),
            archive(&[("base/kanaemi-base.tsv", BASE.as_bytes())]),
            archive(&[("other/kanaemi-base.tsv", BASE.as_bytes())]),
            b"not a zip".to_vec(),
        ] {
            assert!(base.install(&dir, &archive).is_err());
            let left: Vec<_> = fs::read_dir(&dir).unwrap().collect();
            assert!(left.is_empty(), "{left:?}");
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_base_dictionary_goes_before_the_first_official_one() {
        let model = model_bytes();
        let base = entry("base", BASE, Some(&model));
        assert_eq!(
            placed(
                names(&["custom", "skk.tsv", "kanaemi/it/kanaemi-it.kdic", "z.tsv"]),
                &base
            ),
            [
                "custom",
                "skk.tsv",
                "kanaemi/base/kanaemi-base.kdic",
                "kanaemi/it/kanaemi-it.kdic",
                "z.tsv"
            ]
        );
        assert_eq!(
            placed(names(&["custom", "skk.tsv"]), &base),
            ["custom", "skk.tsv", "kanaemi/base/kanaemi-base.kdic"]
        );
    }

    #[test]
    fn an_additional_dictionary_goes_last() {
        let it = entry("it", IT, None);
        assert_eq!(
            placed(names(&["custom", "kanaemi/base/kanaemi-base.kdic"]), &it),
            [
                "custom",
                "kanaemi/base/kanaemi-base.kdic",
                "kanaemi/it/kanaemi-it.kdic"
            ]
        );
    }

    #[test]
    fn a_listed_dictionary_keeps_its_place() {
        let it = entry("it", IT, None);
        let list = names(&["custom", "kanaemi/it/kanaemi-it.tsv", "skk.tsv"]);
        assert_eq!(
            placed(list, &it),
            ["custom", "kanaemi/it/kanaemi-it.kdic", "skk.tsv"]
        );
        let list = names(&["kanaemi/it/kanaemi-it.kdic", "custom"]);
        assert_eq!(placed(list.clone(), &it), list);
    }
}
