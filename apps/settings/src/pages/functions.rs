use std::collections::HashSet;

use kanaemi_functions::{EXTENSION, FunctionError, LuauFunctions, builtin_sources};

use super::*;
use crate::highlight;

/// The functions of the functions folder and the built-in ones, each on or
/// off, with what each says it does and a look at its source.
#[component]
pub fn Functions() -> Element {
    let ctx = use_context::<Ctx>();
    let store = ctx.store.read();
    let folder = store.dir.join(FUNCTIONS_DIR);
    let off: Vec<String> = store
        .state
        .as_ref()
        .ok()
        .map(|l| l.settings.disabled_functions.clone())
        .unwrap_or_default();
    drop(store);
    // Read as the IME reads them, so a file it cannot use says why: again
    // only once a file in the folder changed, as reading runs them.
    let checked = use_hook(|| Rc::new(RefCell::new(None::<(Stamp, Rc<Checked>)>)));
    let Checked { errors, read } = &*{
        let stamp = stamp(&folder);
        let mut checked = checked.borrow_mut();
        match &*checked {
            Some((at, found)) if *at == stamp => found.clone(),
            _ => {
                let found = Rc::new(check(&folder));
                *checked = Some((stamp, found.clone()));
                found
            }
        }
    };
    let builtins: Vec<(&'static str, &'static str)> = builtin_sources().collect();
    let mut items = Vec::new();
    // A file no placeholder can name is no function to turn on or off.
    let mut misnamed = Vec::new();
    for (name, path) in function_files(&folder) {
        let error = errors
            .iter()
            .find(|e| error_path(e) == Some(path.as_path()));
        if matches!(error, Some(FunctionError::Name { .. })) {
            misnamed.push(format!("{name}.{EXTENSION}"));
            continue;
        }
        // A file that gives no function is a module the functions require.
        if error.is_none() && !read.contains(&name) {
            continue;
        }
        let replaces = builtins.iter().any(|(builtin, _)| *builtin == name);
        items.push(ListItem {
            label: name.clone(),
            description: fs::read_to_string(&path).ok().as_deref().and_then(comment),
            meta: replaces.then(|| "組み込みの代わり".to_owned()),
            convert: None,
            warning: error.map(|e| format!("使えません：{e}")),
            chip: None,
            name,
        });
    }
    items.extend(builtins.iter().map(|(name, source)| ListItem {
        name: format!("{BUILTIN_PREFIX}{name}"),
        label: (*name).to_owned(),
        description: comment(source),
        meta: None,
        convert: None,
        warning: None,
        chip: Some("組み込み".to_owned()),
    }));
    let mut looking = use_signal(|| None::<String>);
    let looked = looking().and_then(|name| match name.strip_prefix(BUILTIN_PREFIX) {
        Some(builtin) => builtins
            .iter()
            .find(|(b, _)| *b == builtin)
            .map(|(b, source)| ((*b).to_owned(), (*source).to_owned())),
        None => {
            let file = folder.join(format!("{name}.{EXTENSION}"));
            Some((name, fs::read_to_string(file).unwrap_or_default()))
        }
    });
    rsx! {
        Group {
            title: "関数",
            note: "辞書の表記の {{…}} を埋める関数です。functions フォルダに Luau のファイル（.luau）を置くと、ファイル名の関数としてここに出ます。組み込みの関数と同じ名前のファイルは、組み込みの代わりに使います。スイッチで使うかどうかを決めます。functions フォルダの関数を外すと、同じ名前の組み込みの関数を使います。",
            footer: rsx! {
                button { onclick: move |_| open_folder(&folder), Icon { paths: icons::FOLDER_OPEN } "functions フォルダを開く" }
            },
            SwitchList {
                items,
                off,
                path: path(&["functions", "disabled"]),
                on_info: move |name: String| looking.set(Some(name)),
            }
            for file in misnamed {
                p { class: "item-warning list-error",
                    "{file} は使えません。名前に : ・スペース・{{ }} ・\\ は使えません。"
                }
            }
        }
        if let Some((label, source)) = looked {
            div { class: "modal-backdrop", onclick: move |_| looking.set(None),
                div {
                    class: "modal",
                    onclick: move |e| e.stop_propagation(),
                    header {
                        h2 { "{label}" }
                        button { onclick: move |_| looking.set(None), "閉じる" }
                    }
                    div { class: "modal-body",
                        pre { class: "source",
                            for (class, text) in highlight::luau(&source) {
                                span { class: class.unwrap_or_default(), "{text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What reading the functions of a folder tells.
struct Checked {
    errors: Vec<FunctionError>,
    /// The names of the functions read.
    read: HashSet<String>,
}

fn check(folder: &Path) -> Checked {
    let opened = LuauFunctions::open(folder);
    Checked {
        errors: opened.take_errors(),
        read: opened.names().map(str::to_owned).collect(),
    }
}

/// Every file in `folder` and the folders in it, with when it last changed
/// and how long it is, to tell when to read them again.
type Stamp = Vec<(PathBuf, Option<SystemTime>, u64)>;

fn stamp(folder: &Path) -> Stamp {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    let mut folders = vec![folder.to_owned()];
    while let Some(folder) = folders.pop() {
        // A folder seen already is not entered again, so a link that loops ends.
        if !folder.canonicalize().is_ok_and(|real| seen.insert(real)) {
            continue;
        }
        for path in fs::read_dir(&folder)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
        {
            match fs::metadata(&path) {
                Ok(meta) if meta.is_dir() => folders.push(path),
                Ok(meta) => files.push((path, meta.modified().ok(), meta.len())),
                Err(_) => {}
            }
        }
    }
    files.sort();
    files
}

/// The function files of `folder`: those at its top, by name.
fn function_files(folder: &Path) -> Vec<(String, PathBuf)> {
    let mut files: Vec<(String, PathBuf)> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|e| e == EXTENSION))
        .filter_map(|path| Some((path.file_stem()?.to_str()?.to_owned(), path)))
        .collect();
    files.sort();
    files
}

fn error_path(error: &FunctionError) -> Option<&Path> {
    match error {
        FunctionError::Unreadable { path, .. }
        | FunctionError::Name { path }
        | FunctionError::Unrunnable { path, .. } => Some(path),
        FunctionError::Failed { .. } => None,
    }
}

/// What a function's file says it is: its first line, when that is a comment.
fn comment(source: &str) -> Option<String> {
    let text = source.lines().next()?.strip_prefix("--")?.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_function_says_what_it_is_in_its_first_comment() {
        assert_eq!(
            comment("-- dai: 第 を付ける\nreturn 1").as_deref(),
            Some("dai: 第 を付ける")
        );
        assert_eq!(comment("return function() end"), None);
        assert_eq!(comment("--\nreturn 1"), None);
    }

    #[test]
    fn the_stamp_changes_with_a_module_in_a_folder() {
        let dir = std::env::temp_dir().join(format!(
            "kanaemi-settings-functions-stamp-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("lib")).unwrap();
        fs::write(dir.join("lib").join("m.luau"), "return 1").unwrap();
        let before = stamp(&dir);
        assert_eq!(stamp(&dir), before);
        fs::write(dir.join("lib").join("m.luau"), "return 12").unwrap();
        assert_ne!(stamp(&dir), before);
    }

    #[test]
    fn function_files_are_the_luau_files_at_the_top_of_the_folder() {
        let dir =
            std::env::temp_dir().join(format!("kanaemi-settings-functions-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("lib")).unwrap();
        for name in ["b.luau", "a.luau", "notes.txt", "lib/c.luau"] {
            fs::write(dir.join(name), "").unwrap();
        }
        let names: Vec<String> = function_files(&dir).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["a", "b"]);
    }
}
