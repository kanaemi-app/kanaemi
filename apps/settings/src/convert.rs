//! Text dictionaries made into binary ones, which the IME opens without
//! reading every line.

use std::fs;
use std::path::Path;

use kanaemi_config::binary_name;
use kanaemi_engine::{
    BinaryDictionary, SkkError, convert_text, replace_file, skk_to_text, text_digest,
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

/// Converts the SKK dictionary at `source` into a text dictionary in
/// `folder`, named after it, and returns that name. Its first line says where
/// it came from, which the settings app shows beside it.
pub fn import_skk(source: &Path, folder: &Path) -> Result<String, String> {
    let file_name = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "ファイルの名前が読めません".to_owned())?;
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    let text = format!(
        "# {file_name} から取り込んだ SKK の辞書\n{}",
        skk_to_text(bytes).map_err(|error| match error {
            SkkError::Undecodable { encoding, line } => format!(
                "{line} 行目を {encoding} として読めません。文字コードの指定（coding:）を確かめてください"
            ),
            SkkError::UnsupportedEncoding { name } => {
                format!("文字コード {name} には対応していません")
            }
        })?
    );
    // A file of that name, perhaps an import edited by hand, is kept: the
    // import takes the first free name.
    let name = (1..)
        .map(|n| match n {
            1 => format!("{file_name}.tsv"),
            n => format!("{file_name} {n}.tsv"),
        })
        .find(|name| !folder.join(name).exists())
        .expect("some name is free");
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    replace_file(folder.join(&name), text).map_err(|e| e.to_string())?;
    Ok(name)
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
    fn an_skk_dictionary_is_imported_as_text_under_its_name() {
        let dir = temp_dir("skk");
        let source = dir.join("SKK-JISYO.test");
        fs::write(&source, ";; -*- coding: utf-8 -*-\nきしゃ /記者/汽車/\n").unwrap();
        let folder = dir.join("dictionaries");
        assert_eq!(
            import_skk(&source, &folder),
            Ok("SKK-JISYO.test.tsv".to_owned())
        );
        let text = fs::read_to_string(folder.join("SKK-JISYO.test.tsv")).unwrap();
        assert!(text.starts_with("# SKK-JISYO.test から取り込んだ SKK の辞書\n"));
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

        let imported = import_skk(&source, &folder);

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

        let imported = import_skk(&source, &folder);

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
        import_skk(&source, &folder).unwrap();
        fs::write(folder.join("SKK-JISYO.test.tsv"), "edited\n").unwrap();
        assert_eq!(
            import_skk(&source, &folder),
            Ok("SKK-JISYO.test 2.tsv".to_owned())
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
