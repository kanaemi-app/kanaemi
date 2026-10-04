use super::*;

/// The end of the IME's log, newest first, to read without leaving the app.
#[component]
fn LogView(path: PathBuf, on_close: EventHandler<()>) -> Element {
    let mut filter = use_signal(String::new);
    let mut generation = use_signal(|| 0u32);
    let _ = generation();
    let query = filter();
    let text = logs::tail(&path).unwrap_or_else(|e| format!("読めません：{e}"));
    let lines: Vec<&str> = text
        .lines()
        .rev()
        .filter(|line| query.is_empty() || line.contains(&query))
        .collect();
    let shown = lines.join("\n");
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "modal",
                onclick: move |e| e.stop_propagation(),
                header {
                    div {
                        h2 { "ログ" }
                        p { class: "item-description", "新しい順。末尾の一部だけを示します。" }
                    }
                    div {
                        button { onclick: move |_| generation += 1, "読み直す" }
                        button { onclick: move |_| on_close.call(()), "閉じる" }
                    }
                }
                input {
                    class: "filter",
                    placeholder: "絞り込む…",
                    value: "{filter}",
                    oninput: move |e| filter.set(e.value()),
                }
                div { class: "modal-body",
                    if lines.is_empty() {
                        p { class: "description", "記録はありません" }
                    } else {
                        pre { class: "log", "{shown}" }
                    }
                }
            }
        }
    }
}

const REPOSITORY: &str = "https://github.com/kanaemi-app/kanaemi";

#[component]
pub fn About() -> Element {
    let ctx = use_context::<Ctx>();
    let dir = ctx.store.read().dir.clone();
    let shown = dir.display().to_string();
    let log = kanaemi_config::log_file();
    let mut reading_log = use_signal(|| false);
    rsx! {
        div { class: "about",
            div { class: "logo light", dangerous_inner_html: include_str!("../../assets/logo/kanaemi-icon.svg") }
            div { class: "logo dark", dangerous_inner_html: include_str!("../../assets/logo/kanaemi-icon-dark.svg") }
            h2 { class: "name", "かなえみ" }
            p { class: "version", "バージョン {kanaemi_core::VERSION}" }
            p { class: "tagline", "どの OS でも同じように使える日本語入力" }
        }
        Group { title: "設定のフォルダ",
            div { class: "row",
                div { class: "row-main",
                    code { class: "path", "{shown}" }
                    div { class: "control",
                        button { onclick: move |_| open_folder(&dir), Icon { paths: icons::FOLDER_OPEN } "フォルダを開く" }
                    }
                }
            }
        }
        if let Some(log) = log {
            Group { title: "ログ",
                note: "かなえみが動いた記録です。不具合を報告するときに役立ちます。",
                div { class: "row",
                    div { class: "row-main",
                        code { class: "path", "{log.display()}" }
                        div { class: "control",
                            button { onclick: move |_| reading_log.set(true), "見る" }
                            button {
                                onclick: {
                                    let log = log.clone();
                                    move |_| {
                                        if let Some(folder) = log.parent() {
                                            open_folder(folder);
                                        }
                                    }
                                },
                                Icon { paths: icons::FOLDER_OPEN }
                                "フォルダを開く"
                            }
                        }
                    }
                }
            }
            if reading_log() {
                LogView { path: log.clone(), on_close: move |()| reading_log.set(false) }
            }
        }
        Group {
            div { class: "row",
                div { class: "row-main",
                    span { class: "label", "ソースコード" }
                    div { class: "control",
                        button { onclick: move |_| open_url(REPOSITORY), "GitHub で見る" }
                    }
                }
            }
            div { class: "row",
                div { class: "row-main",
                    span { class: "label", "不具合や要望" }
                    div { class: "control",
                        button { onclick: move |_| open_url(&format!("{REPOSITORY}/issues")), "報告する" }
                    }
                }
            }
        }
        p { class: "credit", "Alisue · MIT License" }
    }
}
