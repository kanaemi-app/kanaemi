use std::fs;
use std::path::PathBuf;

use kanaemi_config::{
    DICTIONARY_DIR, DictionarySource, Settings, binary_name, dictionary_files, dictionary_sources,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kanaemi-config-files-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(DICTIONARY_DIR)).unwrap();
    dir
}

fn files(dir: &std::path::Path, names: &[&str]) -> Vec<DictionarySource> {
    names
        .iter()
        .map(|name| DictionarySource::File(dir.join(DICTIONARY_DIR).join(name)))
        .collect()
}

#[test]
fn without_a_list_the_user_custom_dictionary_comes_first_then_the_folder_by_name() {
    let dir = temp_dir("default");
    for name in ["b.tsv", "a.tsv", "c.kdic", "notes.txt"] {
        fs::write(dir.join(DICTIONARY_DIR).join(name), "").unwrap();
    }

    let sources = dictionary_sources(&dir, None);

    let mut expected = vec![DictionarySource::UserCustom];
    expected.extend(files(&dir, &["a.tsv", "b.tsv", "c.kdic"]));
    assert_eq!(sources, expected);
}

#[test]
fn a_listed_order_is_kept_as_written() {
    let dir = temp_dir("listed");
    let listed = files(&dir, &["b.tsv", "a.tsv"]);

    assert_eq!(dictionary_sources(&dir, Some(&listed)), listed);
}

#[test]
fn every_dictionary_file_in_the_folder_is_listed_by_name() {
    let dir = temp_dir("all");
    for name in ["b.tsv", "a.tsv", "a.kdic", "notes.txt"] {
        fs::write(dir.join(DICTIONARY_DIR).join(name), "").unwrap();
    }

    assert_eq!(
        dictionary_files(dir.join(DICTIONARY_DIR)),
        ["a.kdic", "a.tsv", "b.tsv"]
    );
}

#[test]
fn a_missing_folder_has_no_dictionaries() {
    let dir = std::env::temp_dir().join("kanaemi-config-files-missing-folder");

    assert_eq!(dictionary_files(&dir), Vec::<String>::new());
    assert_eq!(
        dictionary_sources(&dir, None),
        [DictionarySource::UserCustom]
    );
}

#[test]
fn without_a_list_dictionaries_in_sub_folders_are_read_too_in_order_of_their_joined_names() {
    let dir = temp_dir("sub-folders");
    let folder = dir.join(DICTIONARY_DIR);
    fs::create_dir_all(folder.join("sub").join("deeper")).unwrap();
    for name in [
        "b.tsv",
        "sub/deeper/c.tsv",
        "sub/a.kdic",
        "a.tsv",
        "sub/notes.txt",
    ] {
        fs::write(folder.join(name), "").unwrap();
    }

    let sources = dictionary_sources(&dir, None);

    let mut expected = vec![DictionarySource::UserCustom];
    expected.extend(files(
        &dir,
        &["a.tsv", "b.tsv", "sub/a.kdic", "sub/deeper/c.tsv"],
    ));
    assert_eq!(sources, expected);
}

#[test]
fn dictionary_files_in_sub_folders_are_named_with_slashes() {
    let dir = temp_dir("sub-names");
    let folder = dir.join(DICTIONARY_DIR);
    fs::create_dir_all(folder.join("sub")).unwrap();
    fs::write(folder.join("sub").join("a.tsv"), "").unwrap();
    fs::write(folder.join("z.tsv"), "").unwrap();

    assert_eq!(dictionary_files(&folder), ["sub/a.tsv", "z.tsv"]);
}

#[test]
fn a_dictionary_listed_by_its_place_in_a_sub_folder_is_found() {
    let dir = temp_dir("listed-sub");
    let folder = dir.join(DICTIONARY_DIR);
    fs::create_dir_all(folder.join("sub")).unwrap();
    fs::write(folder.join("sub").join("a.tsv"), "").unwrap();
    let listed = dictionary_files(&folder);

    let (settings, problems) = Settings::load(format!("dictionaries = [{:?}]", listed[0]), &dir);

    assert_eq!(problems, []);
    let sources = settings.dictionaries.unwrap();
    let DictionarySource::File(path) = &sources[1] else {
        panic!("{sources:?}");
    };
    assert!(path.is_file(), "{}", path.display());
}

#[test]
fn without_a_list_a_binary_dictionary_stands_for_its_text_one_which_stays_as_a_fallback() {
    let dir = temp_dir("converted");
    let folder = dir.join(DICTIONARY_DIR);
    for name in ["a.tsv", "a.kdic"] {
        fs::write(folder.join(name), "").unwrap();
    }

    let sources = dictionary_sources(&dir, None);

    assert_eq!(
        sources,
        [
            DictionarySource::UserCustom,
            DictionarySource::Converted {
                binary: folder.join("a.kdic"),
                text: folder.join("a.tsv"),
            },
        ]
    );
}

#[test]
fn the_binary_name_swaps_the_extension() {
    assert_eq!(binary_name("a.tsv"), "a.kdic");
    assert_eq!(binary_name("sub/a.tsv"), "sub/a.kdic");
    assert_eq!(binary_name("plain"), "plain.kdic");
}
