//! Controls shared by the pages. Each setting that is off its default offers
//! the way back, naming the default it goes back to.

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;

use crate::Ctx;
use crate::icons::{self, Icon};
use crate::reorder::{drop_side, moved};

/// A field to narrow a list by what is typed, starting empty.
#[component]
pub fn Filter(placeholder: String, oninput: EventHandler<String>) -> Element {
    rsx! {
        input {
            class: "filter",
            placeholder,
            // Never given the value it holds: a render that lags behind the
            // typing writes back an older value, and written while an IME is
            // composing in it, that ends the composing.
            oninput: move |e| oninput.call(e.value()),
        }
    }
}

/// One of the choices a [`Select`] offers.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub value: String,
    pub label: String,
    /// What the choice is, under its name in the open menu.
    pub description: Option<String>,
}

/// A choice among a few: a button the size of the others beside it, opening
/// into a menu that says what each choice is. Drawn by the page, as the OS
/// menu of a `select` keeps its own height and has no room to explain.
#[component]
pub fn Select(choices: Vec<Choice>, value: String, onchange: EventHandler<String>) -> Element {
    // The room above and below the button as the menu opened. The page
    // cannot draw past the window, so the menu opens to the side with the
    // more room and scrolls within it.
    let mut room = use_signal(|| None::<Room>);
    let mut active = use_signal(|| 0);
    let id = use_hook(|| {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        format!("select-{}", NEXT.fetch_add(1, Ordering::Relaxed))
    });
    let chosen = choices.iter().position(|c| c.value == value);
    let label = chosen.map(|i| choices[i].label.clone()).unwrap_or_default();
    let values: Vec<String> = choices.iter().map(|c| c.value.clone()).collect();
    let open = move || room().is_some();
    // Arrow keys move through a menu taller than the window without its
    // scrolling, so the active choice is brought into view.
    let reveal = {
        let id = id.clone();
        move |i: usize| {
            document::eval(&format!(
                "document.getElementById('{id}-{i}')?.scrollIntoView({{block: 'nearest'}})"
            ));
        }
    };
    let show = {
        let id = id.clone();
        move || {
            active.set(chosen.unwrap_or(0));
            let script = format!(
                "const r = document.getElementById('{id}').getBoundingClientRect(); \
                 return [r.top, r.bottom, window.innerWidth - r.right, r.width, window.innerHeight];"
            );
            spawn(async move {
                let Ok((top, bottom, right, width, height)) = document::eval(&script)
                    .join::<(f64, f64, f64, f64, f64)>()
                    .await
                else {
                    return;
                };
                room.set(Some(Room {
                    top,
                    bottom,
                    right,
                    width,
                    height,
                }));
            });
        }
    };
    let menu_style = room().map(|room| room.style()).unwrap_or_default();
    rsx! {
        div {
            id,
            class: "select",
            onkeydown: {
                let mut show = show.clone();
                move |e: KeyboardEvent| {
                    let count = values.len();
                    if count == 0 {
                        return;
                    }
                    match e.key() {
                        Key::ArrowDown | Key::ArrowUp if !open() => show(),
                        Key::ArrowDown | Key::ArrowUp => {
                            let next = if e.key() == Key::ArrowUp {
                                (active() + count - 1) % count
                            } else {
                                (active() + 1) % count
                            };
                            active.set(next);
                            reveal(next);
                        }
                        Key::Enter if open() => {
                            onchange.call(values[active()].clone());
                            room.set(None);
                        }
                        // Kept from a modal around it, which Escape closes.
                        Key::Escape if open() => room.set(None),
                        _ => return,
                    }
                    e.prevent_default();
                    e.stop_propagation();
                }
            },
            button {
                class: "select-button",
                "aria-haspopup": "listbox",
                "aria-expanded": open(),
                // Tab to another control leaves the menu behind otherwise.
                onblur: move |_| room.set(None),
                onclick: {
                    let mut show = show.clone();
                    move |_| {
                        if open() {
                            room.set(None);
                        } else {
                            show();
                        }
                    }
                },
                span { "{label}" }
                Icon { paths: icons::SELECTOR }
            }
            if open() {
                div {
                    class: "select-backdrop",
                    onclick: move |_| room.set(None),
                    // The page would scroll away under a menu placed on the window.
                    onwheel: move |_| room.set(None),
                }
                div {
                    class: "select-menu",
                    role: "listbox",
                    style: "{menu_style}",
                    // Keeps the focus on the button, whose losing it closes the menu.
                    onmousedown: move |e| e.prevent_default(),
                    for (i , choice) in choices.into_iter().enumerate() {
                        div {
                            key: "{choice.value}",
                            id: "{id}-{i}",
                            class: if i == active() { "select-choice active" } else { "select-choice" },
                            role: "option",
                            "aria-selected": chosen == Some(i),
                            onmouseenter: move |_| active.set(i),
                            onclick: {
                                let value = choice.value.clone();
                                move |_| {
                                    onchange.call(value.clone());
                                    room.set(None);
                                }
                            },
                            span { class: "select-check",
                                if chosen == Some(i) {
                                    Icon { paths: icons::CHECK }
                                }
                            }
                            span { class: "select-choice-text",
                                span { class: "label", "{choice.label}" }
                                if let Some(description) = choice.description {
                                    span { class: "description", "{description}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Where a [`Select`]'s button is in the window as its menu opens, in
/// pixels: its top and bottom, the room right of it, its width, and the window's
/// height.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Room {
    top: f64,
    bottom: f64,
    right: f64,
    width: f64,
    height: f64,
}

impl Room {
    /// The menu placed on the window rather than in the scrolled page, so
    /// only the window's edges bound it: right-aligned with the button,
    /// below it unless that is short of a menu of a few choices and above
    /// has more, and no taller than the side it opens to.
    fn style(self) -> String {
        const ENOUGH: f64 = 320.0;
        const GAP: f64 = 4.0;
        const MARGIN: f64 = 8.0;
        let (above, below) = (self.top, self.height - self.bottom);
        let upward = below < ENOUGH && above > below;
        let side = if upward { above } else { below };
        let max = (side - GAP - MARGIN).max(80.0);
        let (right, width) = (self.right, self.width);
        if upward {
            let from_bottom = self.height - self.top + GAP;
            format!(
                "right: {right}px; min-width: {width}px; bottom: {from_bottom}px; max-height: {max}px;"
            )
        } else {
            let top = self.bottom + GAP;
            format!("right: {right}px; min-width: {width}px; top: {top}px; max-height: {max}px;")
        }
    }
}

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
                if shipped.is_empty() {
                    "既定の空に戻す"
                } else {
                    "既定の「{shipped}」に戻す"
                }
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
    /// Where it comes from, beside its name, such as built into Kanaemi.
    pub chip: Option<String>,
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
                chip: None,
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
            span { class: "name",
                "{item.label}"
                if let Some(chip) = &item.chip {
                    span { class: "chip-kind", "{chip}" }
                }
            }
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

/// Items each on or off, in no order: what is off is written to `path` as a
/// list of names. With `on_info`, each row offers a look inside.
#[component]
pub fn SwitchList(
    items: Vec<ListItem>,
    off: Vec<String>,
    path: Vec<String>,
    on_info: Option<EventHandler<String>>,
) -> Element {
    let ctx = use_context::<Ctx>();
    let error = ctx.errors.read().get(&path.join(".")).cloned();
    let write = move |names: Vec<String>| {
        let path: Vec<&str> = path.iter().map(String::as_str).collect();
        ctx.change(&path, Some(names.into_iter().collect()));
    };
    rsx! {
        ul { class: "ordered",
            for item in items {
                li {
                    key: "{item.name}",
                    class: if off.contains(&item.name) { "off" } else { "" },
                    ItemBody { item: item.clone(), on_info, on_convert: None }
                    input {
                        class: "switch",
                        r#type: "checkbox",
                        checked: !off.contains(&item.name),
                        onchange: {
                            let off = off.clone();
                            let write = write.clone();
                            let name = item.name.clone();
                            move |_| {
                                let mut names: Vec<String> =
                                    off.iter().filter(|n| **n != name).cloned().collect();
                                if !off.contains(&name) {
                                    names.push(name.clone());
                                }
                                write(names);
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
