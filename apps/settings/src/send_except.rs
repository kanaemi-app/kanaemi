//! The applications the keys sent in place of others are not sent in: shown
//! by their names and icons, and picked from those running or installed
//! where the OS tells them, written by name elsewhere.

use dioxus::prelude::*;

use crate::Ctx;
use crate::apps::{self, App};
use crate::controls::{Group, ResetLine};
use crate::icons::{self, Icon};

const ITEM: &str = "keys.send_except";

#[component]
pub fn SendExcept(apps: Vec<String>) -> Element {
    let ctx = use_context::<Ctx>();
    let mut picking = use_signal(|| None::<Vec<App>>);
    // Whether it was written; a refusal is shown under the list.
    let write = move |apps: Vec<String>| {
        ctx.change(&["keys", "send_except"], Some(apps.into_iter().collect()));
        !ctx.errors.peek().contains_key(ITEM)
    };
    let error = ctx.errors.read().get(ITEM).cloned();
    let add = {
        let apps = apps.clone();
        move |id: String| {
            let id = id.trim().to_owned();
            if id.is_empty() || apps.iter().any(|a| a.eq_ignore_ascii_case(&id)) {
                return true;
            }
            write(apps.iter().cloned().chain([id]).collect())
        }
    };
    let shown: Vec<App> = apps.iter().map(|id| apps::describe(id)).collect();
    rsx! {
        Group {
            title: "キーを置き換えないアプリ",
            note: "ここに並べたアプリでは、「ふだん」のキーの置き換えをしません。",
            footer: rsx! {
                if apps::PICKABLE {
                    button {
                        onclick: move |_| picking.set(Some(apps::running())),
                        Icon { paths: icons::PLUS }
                        "アプリを足す…"
                    }
                } else {
                    span {}
                }
                ResetLine {
                    shipped: (!apps.is_empty()).then(String::new),
                    path: vec!["keys".to_owned(), "send_except".to_owned()],
                }
            },
            for app in shown {
                div { class: "row app-row", key: "{app.id}",
                    AppIcon { app: app.clone() }
                    div { class: "row-text",
                        span { class: "label", "{app.name}" }
                        if app.name != app.id {
                            span { class: "description app-id", "{app.id}" }
                        }
                    }
                    button {
                        class: "icon-button",
                        title: "外す",
                        "aria-label": "{app.name} を外す",
                        onclick: {
                            let apps = apps.clone();
                            let id = app.id.clone();
                            move |_| {
                                write(apps.iter().filter(|a| **a != id).cloned().collect());
                            }
                        },
                        Icon { paths: icons::X }
                    }
                }
            }
            if apps.is_empty() {
                div { class: "row",
                    span { class: "none", "なし（どのアプリでも置き換えます）" }
                }
            }
            if !apps::PICKABLE {
                NameField { on_add: add.clone() }
            }
            if let Some(error) = error {
                div { class: "row",
                    p { class: "error", "この値は使えません：{error}" }
                }
            }
        }
        if let Some(running) = picking() {
            AppPicker {
                running,
                chosen: apps.clone(),
                on_pick: {
                    let add = add.clone();
                    move |id: String| {
                        if add(id) {
                            picking.set(None);
                        }
                    }
                },
                on_close: move |_| picking.set(None),
            }
        }
    }
}

/// The application's icon, or a blank of its size.
#[component]
fn AppIcon(app: App) -> Element {
    rsx! {
        if let Some(icon) = app.icon {
            img { class: "app-icon", src: "{icon}", alt: "" }
        } else {
            span { class: "app-icon blank" }
        }
    }
}

/// The applications running to pick one from, or any installed one.
#[component]
fn AppPicker(
    running: Vec<App>,
    chosen: Vec<String>,
    on_pick: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let mut filter = use_signal(String::new);
    let query = filter().to_lowercase();
    let listed: Vec<(App, bool)> = running
        .into_iter()
        .filter(|app| {
            query.is_empty()
                || app.name.to_lowercase().contains(&query)
                || app.id.to_lowercase().contains(&query)
        })
        .map(|app| {
            let added = chosen.iter().any(|c| c.eq_ignore_ascii_case(&app.id));
            (app, added)
        })
        .collect();
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "modal",
                onclick: move |e| e.stop_propagation(),
                header {
                    div {
                        h2 { "アプリを足す" }
                        p { class: "item-description",
                            "動いているアプリから選びます。ないときは「ほかのアプリを選ぶ…」で、アプリケーションフォルダから選べます。"
                        }
                    }
                    button { onclick: move |_| on_close.call(()), "閉じる" }
                }
                input {
                    class: "filter",
                    placeholder: "名前で絞り込む…",
                    value: "{filter}",
                    oninput: move |e| filter.set(e.value()),
                }
                div { class: "modal-body",
                    for (app , added) in listed {
                        button {
                            class: "app-choice",
                            key: "{app.id}",
                            disabled: added,
                            onclick: {
                                let id = app.id.clone();
                                move |_| on_pick.call(id.clone())
                            },
                            AppIcon { app: app.clone() }
                            span { class: "app-choice-text",
                                span { class: "label", "{app.name}" }
                                span { class: "description app-id", "{app.id}" }
                            }
                            if added {
                                span { class: "chip-where", "足してあります" }
                            }
                        }
                    }
                }
                div { class: "modal-footer",
                    button {
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(app) = apps::choose().await {
                                    on_pick.call(app.id);
                                }
                            });
                        },
                        "ほかのアプリを選ぶ…"
                    }
                }
            }
        }
    }
}

/// Where applications cannot be picked: one written by the name the IME
/// knows it by.
#[component]
fn NameField(on_add: Callback<String, bool>) -> Element {
    let mut adding = use_signal(String::new);
    let mut add = move || {
        if on_add.call(adding()) {
            adding.set(String::new());
        }
    };
    rsx! {
        div { class: "row",
            span { class: "description",
                "Windows では実行ファイル名（WindowsTerminal.exe）、Linux では GTK のプログラム名（ghostty）で書きます。大文字と小文字は区別しません。"
            }
            div { class: "except-add",
                input {
                    r#type: "text",
                    placeholder: "アプリの名前",
                    value: "{adding}",
                    oninput: move |e| adding.set(e.value()),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter {
                            add();
                        }
                    },
                }
                button {
                    disabled: adding().trim().is_empty(),
                    onclick: move |_| add(),
                    "足す"
                }
            }
        }
    }
}
