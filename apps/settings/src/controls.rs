//! Controls shared by the pages. Each setting that is off its default offers
//! the way back, naming the default it goes back to.

use dioxus::prelude::*;

use crate::Ctx;
use crate::icons::{self, Icon};
use crate::reorder::{drop_side, moved};

/// Settings that belong together, in one rounded box under a heading.
#[component]
pub fn Group(
    title: Option<String>,
    note: Option<String>,
    children: Element,
    /// Below the box, as a button that opens a folder.
    footer: Option<Element>,
) -> Element {
    rsx! {
        section { class: "group",
            if let Some(title) = title {
                h2 { "{title}" }
            }
            if let Some(note) = note {
                p { class: "note", "{note}" }
            }
            div { class: "box", {children} }
            if let Some(footer) = footer {
                div { class: "footer", {footer} }
            }
        }
    }
}

/// One setting: its name and description, the control, and below them why a
/// change was refused and the way back to the default.
#[component]
pub fn Row(
    label: String,
    description: Option<String>,
    /// The path of the setting in the file.
    path: Vec<String>,
    /// How the default reads, while the setting is off it.
    shipped: Option<String>,
    children: Element,
) -> Element {
    let ctx = use_context::<Ctx>();
    let item = path.join(".");
    let error = ctx.errors.read().get(&item).cloned();
    rsx! {
        div { class: "row",
            div { class: "row-main",
                div { class: "row-text",
                    span { class: "label", "{label}" }
                    if let Some(description) = description {
                        span { class: "description", "{description}" }
                    }
                }
                div { class: "control", {children} }
            }
            if let Some(error) = error {
                p { class: "error", "この値は使えません：{error}" }
            }
            ResetLine { shipped, path }
        }
    }
}

#[component]
pub fn ResetLine(shipped: Option<String>, path: Vec<String>) -> Element {
    let ctx = use_context::<Ctx>();
    let Some(shipped) = shipped else {
        return rsx! {};
    };
    rsx! {
        div { class: "reset-line",
            button {
                class: "reset",
                onclick: move |_| {
                    let path: Vec<&str> = path.iter().map(String::as_str).collect();
                    ctx.change(&path, None);
                },
                "既定の「{shipped}」に戻す"
            }
        }
    }
}

/// One entry of an [`OrderedList`]: its name in the settings file, and what
/// is shown for it.
#[derive(Clone, PartialEq)]
pub struct ListItem {
    pub name: String,
    pub label: String,
    /// What the file says it is, from its first comment line.
    pub description: Option<String>,
    /// A short fact beside it, such as the file's size.
    pub meta: Option<String>,
    /// The label of the button that makes it a binary dictionary, if one.
    pub convert: Option<String>,
    /// What is wrong with it, such as lines the IME cannot read.
    pub warning: Option<String>,
}

/// Items in a chosen order, each on or off: the order is the order of use,
/// changed by dragging a row by its handle. `fixed` items cannot be turned
/// off, only moved. With `on_info`, each row offers a look inside; with
/// `on_convert`, each convertible row offers to become a binary dictionary.
#[component]
pub fn OrderedList(
    items: Vec<ListItem>,
    chosen: Vec<String>,
    fixed: Vec<String>,
    path: Vec<String>,
    on_info: Option<EventHandler<String>>,
    on_convert: Option<EventHandler<String>>,
) -> Element {
    let ctx = use_context::<Ctx>();
    let mut dragging = use_signal(|| None::<usize>);
    let mut target = use_signal(|| None::<usize>);
    let find = |name: &str| {
        items
            .iter()
            .find(|i| i.name == name)
            .cloned()
            .unwrap_or(ListItem {
                name: name.to_owned(),
                label: name.to_owned(),
                description: None,
                meta: None,
                convert: None,
                warning: None,
            })
    };
    let rest: Vec<ListItem> = items
        .iter()
        .filter(|i| !chosen.contains(&i.name))
        .cloned()
        .collect();
    // Why the last change was refused, such as a table with lines the IME
    // cannot read.
    let error = ctx.errors.read().get(&path.join(".")).cloned();
    let write = move |names: Vec<String>| {
        let path: Vec<&str> = path.iter().map(String::as_str).collect();
        ctx.change(&path, Some(names.into_iter().collect()));
    };
    // Pressing, moving and releasing the mouse rather than HTML drag and
    // drop, which the platforms' web views do not support alike.
    rsx! {
        ol {
            class: if dragging().is_some() { "ordered moving" } else { "ordered" },
            onmouseup: {
                let chosen = chosen.clone();
                let write = write.clone();
                move |_| {
                    if let (Some(from), Some(to)) = (dragging.take(), target.take())
                        && from != to
                    {
                        write(moved(&chosen, from, to));
                    }
                }
            },
            onmouseleave: move |_| {
                dragging.set(None);
                target.set(None);
            },
            for (i , item) in chosen.iter().map(|n| find(n)).enumerate() {
                li {
                    key: "{item.name}",
                    class: "{row_class(dragging(), target(), i)}",
                    onmouseenter: move |_| {
                        if dragging().is_some() {
                            target.set(Some(i));
                        }
                    },
                    span {
                        class: "handle",
                        title: "ドラッグして並べ替える",
                        onmousedown: move |e: MouseEvent| {
                            e.prevent_default();
                            dragging.set(Some(i));
                            target.set(Some(i));
                        },
                        Icon { paths: icons::GRIP_VERTICAL }
                    }
                    ItemBody { item: item.clone(), on_info, on_convert }
                    input {
                        class: "switch",
                        r#type: "checkbox",
                        checked: true,
                        disabled: fixed.contains(&item.name),
                        onchange: {
                            let chosen = chosen.clone();
                            let write = write.clone();
                            let name = item.name.clone();
                            move |_| write(chosen.iter().filter(|n| **n != name).cloned().collect())
                        },
                    }
                }
            }
            for item in rest {
                li { key: "{item.name}", class: "off",
                    span { class: "handle" }
                    ItemBody { item: item.clone(), on_info, on_convert }
                    input {
                        class: "switch",
                        r#type: "checkbox",
                        checked: false,
                        onchange: {
                            let mut chosen = chosen.clone();
                            let write = write.clone();
                            let name = item.name.clone();
                            move |_| {
                                chosen.push(name.clone());
                                write(chosen.clone());
                            }
                        },
                    }
                }
            }
        }
        if let Some(error) = error {
            p { class: "error list-error", "使えません：{error}" }
        }
    }
}

#[component]
fn ItemBody(
    item: ListItem,
    on_info: Option<EventHandler<String>>,
    on_convert: Option<EventHandler<String>>,
) -> Element {
    rsx! {
        div { class: "item-text",
            span { class: "name", "{item.label}" }
            if let Some(description) = &item.description {
                span { class: "item-description", "{description}" }
            }
            if let Some(warning) = &item.warning {
                span { class: "item-warning", "{warning}" }
            }
        }
        if let Some(meta) = &item.meta {
            span { class: "meta", "{meta}" }
        }
        if let (Some(on_convert), Some(label)) = (on_convert, &item.convert) {
            button {
                class: "info",
                title: "速く開けるバイナリの辞書を、同じ名前の .kdic で作って使います",
                onclick: {
                    let name = item.name.clone();
                    move |_| on_convert.call(name.clone())
                },
                "{label}"
            }
        }
        if let Some(on_info) = on_info {
            button {
                class: "info",
                title: "中身を見る",
                "aria-label": "中身を見る",
                onclick: {
                    let name = item.name.clone();
                    move |_| on_info.call(name.clone())
                },
                Icon { paths: icons::LIST_SEARCH }
            }
        }
    }
}

/// A modifier key, by the sign on it and its name, turned on or off.
#[component]
pub fn KeyToggle(
    symbol: &'static str,
    name: &'static str,
    /// What turning it on lets through, under the key.
    what: &'static str,
    on: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "row",
            div { class: "row-main",
                span { class: "key-badge modifier", "{symbol}" }
                div { class: "row-text",
                    span { class: "label", "{name}" }
                    span { class: "description", "{what}" }
                }
                input {
                    class: "switch",
                    r#type: "checkbox",
                    role: "switch",
                    checked: on,
                    onchange: move |_| onclick.call(()),
                }
            }
        }
    }
}

/// A row's look while a row is dragged: the one in the air, and the line
/// where it would land.
fn row_class(dragging: Option<usize>, target: Option<usize>, row: usize) -> &'static str {
    match drop_side(dragging, target, row) {
        Some(true) => "drop-after",
        Some(false) => "drop-before",
        None if dragging == Some(row) => "dragging",
        None => "",
    }
}
