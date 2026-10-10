//! The words registered in the user custom dictionary, to add to, edit and
//! remove where the dictionary is looked into. What may be written, and how,
//! is the engine's: this only shows it.

use kanaemi_engine::{UserWord, add_user_word, edit_user_word, remove_user_word, user_words};
use kanaemi_functions::builtin_sources;

use super::functions::function_files;
use super::*;
use crate::messages::describe_word;

/// How many words the list shows at most, so a long dictionary opens at
/// once; the filter finds the rest.
const WORDS_SHOWN: usize = 200;

/// What the form below a word, or below the list, is open for.
#[derive(Clone, PartialEq)]
enum Form {
    Adding,
    Editing(UserWord),
}

/// The user custom dictionary as its button to look into a dictionary opens
/// it: the words to edit in place of the entries the other dictionaries show.
#[component]
pub(super) fn UserWords(
    custom: PathBuf,
    functions: PathBuf,
    on_close: EventHandler<()>,
) -> Element {
    let mut ctx = use_context::<Ctx>();
    // The store is read again when the window comes back, as the IME may
    // have registered more meanwhile, and is written after each change, so
    // every part of the page that reads the file reads it again.
    let _ = ctx.store.read();
    let mut form = use_signal(|| None::<Form>);
    let mut filter = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let words = user_words(&custom).map_err(|e| e.to_string());
    let query: String = filter().trim().nfc().collect();
    let (shown, more) = match &words {
        Ok(words) => {
            let matching: Vec<&UserWord> = words.iter().filter(|w| found_by(w, &query)).collect();
            let more = matching.len().saturating_sub(WORDS_SHOWN);
            let shown: Vec<UserWord> = matching.into_iter().take(WORDS_SHOWN).cloned().collect();
            (shown, more)
        }
        Err(_) => (Vec::new(), 0),
    };
    let any = words.as_ref().is_ok_and(|w| !w.is_empty());
    let done = move |_| {
        form.set(None);
        ctx.store.write();
    };
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "modal user-words",
                onclick: move |e| e.stop_propagation(),
                header {
                    div {
                        h2 { "ユーザー辞書" }
                        p { class: "item-description",
                            "登録した語です。語を足したり、読みと表記を直したりできます。送り仮名のある語は、読みの送り仮名の前に * を入れ（か*く）、表記は送り仮名まで書きます（書く）。数は読みに {{}} と書き、表記の {{kanji}} のような置き場所で書き方を決めます。"
                        }
                    }
                    button { onclick: move |_| on_close.call(()), "閉じる" }
                }
                if any {
                    input {
                        class: "filter",
                        placeholder: "読みで絞り込む…",
                        // Not given the value it holds: written back while an
                        // IME is composing in it, it would end the composing.
                        oninput: move |e| filter.set(e.value()),
                    }
                }
                div { class: "modal-body",
                    match &words {
                        Err(e) => rsx! { p { class: "error", "ユーザー辞書を読めません：{e}" } },
                        Ok(_) if shown.is_empty() => rsx! {
                            p { class: "description",
                                if any {
                                    "合う語はありません"
                                } else {
                                    "語はありません"
                                }
                            }
                        },
                        Ok(_) => rsx! {
                            table { class: "rules words",
                                thead {
                                    tr {
                                        th { "読み" }
                                        th { "表記" }
                                        th {}
                                    }
                                }
                                tbody {
                                    for word in shown {
                                        if form() == Some(Form::Editing(word.clone())) {
                                            tr { key: "{word.reading()}\t{word.surface()}",
                                                td { colspan: 3,
                                                    WordForm {
                                                        original: Some(word.clone()),
                                                        custom: custom.clone(),
                                                        functions: functions.clone(),
                                                        on_done: done,
                                                        on_cancel: move |_| form.set(None),
                                                    }
                                                }
                                            }
                                        } else {
                                            tr { key: "{word.reading()}\t{word.surface()}",
                                                td { "{word.reading()}" }
                                                td { "{word.surface()}" }
                                                td { class: "word-actions",
                                                    button {
                                                        onclick: {
                                                            let word = word.clone();
                                                            move |_| form.set(Some(Form::Editing(word.clone())))
                                                        },
                                                        "直す"
                                                    }
                                                    button {
                                                        onclick: {
                                                            let custom = custom.clone();
                                                            let word = word.clone();
                                                            move |_| {
                                                                match remove_user_word(&custom, &word) {
                                                                    Ok(()) => error.set(None),
                                                                    Err(e) => error.set(Some(format!("消せません：{e}"))),
                                                                }
                                                                ctx.store.write();
                                                            }
                                                        },
                                                        "消す"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }
                    if more > 0 {
                        p { class: "description",
                            "ほかに {more} 語あります。読みで絞り込んでください。"
                        }
                    }
                    if form() == Some(Form::Adding) {
                        WordForm {
                            original: None,
                            custom: custom.clone(),
                            functions: functions.clone(),
                            on_done: done,
                            on_cancel: move |_| form.set(None),
                        }
                    } else {
                        button {
                            class: "add",
                            onclick: move |_| form.set(Some(Form::Adding)),
                            "語を足す"
                        }
                    }
                    if let Some(error) = error() {
                        p { class: "error", "{error}" }
                    }
                }
            }
        }
    }
}

/// The reading and the surface of a word to add, or of `original` to edit,
/// written only when the engine takes them.
#[component]
fn WordForm(
    original: Option<UserWord>,
    custom: PathBuf,
    functions: PathBuf,
    on_done: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    // The fields start from the word and are never given a value again:
    // written back while an IME is composing in them, it would end the
    // composing.
    let initial = use_hook(|| {
        original
            .as_ref()
            .map(|word| (word.reading(), word.surface()))
            .unwrap_or_default()
    });
    let mut reading = use_signal(|| initial.0.clone());
    let mut surface = use_signal(|| initial.1.clone());
    let mut error = use_signal(|| None::<String>);
    let save = {
        let original = original.clone();
        move |_| {
            let written = UserWord::new(&reading(), &surface(), |name| {
                has_function(&functions, name)
            })
            .and_then(|word| match &original {
                Some(old) => edit_user_word(&custom, old, &word),
                None => add_user_word(&custom, &word),
            });
            match written {
                Ok(()) => on_done.call(()),
                Err(e) => error.set(Some(describe_word(&e))),
            }
        }
    };
    let blank = reading().trim().is_empty() && surface().trim().is_empty();
    rsx! {
        div {
            class: "binding-form word-form",
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    on_cancel.call(());
                }
            },
            div { class: "binding-form-row",
                label { class: "word-field",
                    span { "読み" }
                    input {
                        placeholder: "か*く",
                        initial_value: "{initial.0}",
                        oninput: move |e| reading.set(e.value()),
                    }
                }
                label { class: "word-field",
                    span { "表記" }
                    input {
                        placeholder: "書く",
                        initial_value: "{initial.1}",
                        oninput: move |e| surface.set(e.value()),
                    }
                }
            }
            if let Some(message) = error() {
                p { class: "error", "{message}" }
            }
            div { class: "binding-form-buttons",
                button { class: "primary", disabled: blank, onclick: save,
                    if original.is_some() {
                        "保存"
                    } else {
                        "足す"
                    }
                }
                button { onclick: move |_| on_cancel.call(()), "キャンセル" }
            }
        }
    }
}

/// Whether `word` is found by `query` (trimmed and NFC): its reading starts
/// with it, as the other dictionaries are looked into, which have no index of
/// their surfaces.
fn found_by(word: &UserWord, query: &str) -> bool {
    word.reading().starts_with(query)
}

/// Whether a placeholder may name `name`: a built-in function, or one of the
/// functions folder, on or off. Whether it runs, the functions page tells.
fn has_function(folder: &Path, name: &str) -> bool {
    builtin_sources().any(|(builtin, _)| builtin == name)
        || function_files(folder).iter().any(|(file, _)| file == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_narrowed_to_the_readings_the_query_starts_as_other_dictionaries_are() {
        let word = |reading, surface| UserWord::new(reading, surface, |_| true).unwrap();
        assert!(found_by(&word("かんじ", "漢字"), "かん"));
        assert!(
            !found_by(&word("かんじ", "漢字"), "んじ"),
            "not anywhere in the reading"
        );
        assert!(
            !found_by(&word("かんじ", "漢字"), "漢"),
            "not by the surface"
        );
        assert!(found_by(&word("か*く", "書く"), "か*く"));
        assert!(found_by(&word("かんじ", "漢字"), ""));
    }

    #[test]
    fn a_function_is_built_in_or_in_the_functions_folder() {
        let dir =
            std::env::temp_dir().join(format!("kanaemi-settings-words-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("mine.luau"), "").unwrap();
        let builtin = builtin_sources().next().unwrap().0;

        assert!(has_function(&dir, builtin));
        assert!(has_function(&dir, "mine"));
        assert!(!has_function(&dir, "笑"));
        let _ = fs::remove_dir_all(&dir);
    }
}
