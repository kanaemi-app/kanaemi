//! The key bindings page: keys by what they do, each set once for every
//! scene, and, for those who want it, a table per scene after Arto's, a row
//! opening a form in place, and keys entered by pressing them.

use dioxus::prelude::*;
use kanaemi_config::{
    APPLICATION_TABLE, UNBOUND, Value, actions, bindings_table, format_action, parse_action,
    parse_binding_key, parse_chord, sendable_keys,
};
use kanaemi_core::{Action, Bindings, Config, Form};

use crate::Ctx;
use crate::complete::KeyInput;
use crate::icons::{self, Icon};
use crate::intents::{self, Change, GROUPS, Intent};

/// The modifiers held with a recorded key.
#[derive(Clone, Copy, Default)]
pub struct Held {
    pub ctrl: bool,
    pub cmd: bool,
    pub alt: bool,
    pub shift: bool,
}

/// A pressed key as the settings file writes it, from the web view's `key`
/// and `code`; `None` for a modifier pressed on its own.
pub fn recorded(held: Held, key: &str, code: &str) -> Option<String> {
    let mut shift = held.shift;
    let name = match key {
        "Shift" | "Control" | "Alt" | "Meta" | "CapsLock" => return None,
        "Enter" => "enter".to_owned(),
        "Escape" => "esc".to_owned(),
        "Backspace" => "backspace".to_owned(),
        "Delete" => "delete".to_owned(),
        " " => "space".to_owned(),
        "ArrowLeft" => "left".to_owned(),
        "ArrowRight" => "right".to_owned(),
        "ArrowUp" => "up".to_owned(),
        "ArrowDown" => "down".to_owned(),
        "Home" => "home".to_owned(),
        "End" => "end".to_owned(),
        "Alphanumeric" | "Eisu" | "Lang2" => "eisu".to_owned(),
        "KanaMode" | "Lang1" => "kana".to_owned(),
        "Convert" => "henkan".to_owned(),
        "NonConvert" => "muhenkan".to_owned(),
        _ if key.len() > 1 && key.starts_with('F') && key[1..].parse::<u8>().is_ok() => {
            key.to_lowercase()
        }
        // A letter is what it types, wherever the layout puts it; a capital
        // without Shift (Caps Lock) stays one, as the IME gets it.
        _ if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic()) => {
            if shift {
                key.to_lowercase()
            } else {
                key.to_owned()
            }
        }
        // Option changes what a letter types (ƒ for f): its place tells it.
        _ if code.starts_with("Key") && code.len() == 4 => code[3..].to_lowercase(),
        // So it does with a digit or a symbol (™ for 2).
        _ if held.alt && unshifted(code).is_some() => {
            let symbol = if shift {
                shifted(code)
            } else {
                unshifted(code)
            };
            shift = false;
            symbol?.to_string()
        }
        _ => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                // A symbol typed with Shift is the symbol itself (`:`).
                (Some(c), None) => {
                    shift = false;
                    c.to_string()
                }
                _ => return None,
            }
        }
    };
    let names = [
        (held.ctrl, "ctrl+"),
        (held.cmd, "cmd+"),
        (held.alt, "alt+"),
        (shift, "shift+"),
    ];
    let mut text: String = names
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, n)| *n)
        .collect();
    text.push_str(&name);
    parse_chord(&text).map(|_| text)
}

/// How a key is pressed for its binding to act, as the settings file writes
/// it after the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Press {
    Down,
    Tap,
    Hold,
}

impl Press {
    const ALL: [Press; 3] = [Press::Down, Press::Tap, Press::Hold];

    fn suffix(self) -> &'static str {
        match self {
            Press::Down => "",
            Press::Tap => "#tap",
            Press::Hold => "#hold",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Press::Down => "押したとき",
            Press::Tap => "単独押し",
            Press::Hold => "押さえたまま",
        }
    }

    fn written(self, key: &str) -> String {
        format!("{key}{}", self.suffix())
    }
}

/// A key as the settings file writes it, apart from how it is pressed.
fn split_press(text: &str) -> (&str, Press) {
    for press in [Press::Tap, Press::Hold] {
        if let Some(key) = text.strip_suffix(press.suffix())
            && !key.is_empty()
        {
            return (key, press);
        }
    }
    (text, Press::Down)
}

/// The ways `key` can be pressed for a binding.
fn presses(key: &str) -> Vec<Press> {
    Press::ALL
        .into_iter()
        .filter(|p| parse_binding_key(p.written(key)).is_some())
        .collect()
}

/// A modifier key by the web view's `code`, for when it is pressed alone.
fn recorded_alone(code: &str) -> Option<&'static str> {
    Some(match code {
        "ShiftLeft" => "left-shift",
        "ShiftRight" => "right-shift",
        "ControlLeft" => "left-ctrl",
        "ControlRight" => "right-ctrl",
        "MetaLeft" => "left-cmd",
        "MetaRight" => "right-cmd",
        "AltLeft" => "left-alt",
        "AltRight" => "right-alt",
        _ => return None,
    })
}

const SELECT_LABELS: [&str; 9] = [
    "1 番目の候補を確定",
    "2 番目の候補を確定",
    "3 番目の候補を確定",
    "4 番目の候補を確定",
    "5 番目の候補を確定",
    "6 番目の候補を確定",
    "7 番目の候補を確定",
    "8 番目の候補を確定",
    "9 番目の候補を確定",
];

/// The character a digit or symbol key types with Shift, on a US layout.
fn shifted(code: &str) -> Option<char> {
    const DIGITS: &str = ")!@#$%^&*(";
    if let Some(digit) = code.strip_prefix("Digit") {
        let n: usize = digit.parse().ok()?;
        return DIGITS.chars().nth(n);
    }
    Some(match code {
        "Minus" => '_',
        "Equal" => '+',
        "BracketLeft" => '{',
        "BracketRight" => '}',
        "Backslash" => '|',
        "Semicolon" => ':',
        "Quote" => '"',
        "Comma" => '<',
        "Period" => '>',
        "Slash" => '?',
        "Backquote" => '~',
        _ => return None,
    })
}

/// The character of a digit or symbol key by its place, as the IME gets it
/// with Option held.
fn unshifted(code: &str) -> Option<char> {
    if let Some(digit) = code.strip_prefix("Digit") {
        return digit.chars().next();
    }
    Some(match code {
        "Minus" => '-',
        "Equal" => '=',
        "BracketLeft" => '[',
        "BracketRight" => ']',
        "Backslash" => '\\',
        "Semicolon" => ';',
        "Quote" => '\'',
        "Comma" => ',',
        "Period" => '.',
        "Slash" => '/',
        "Backquote" => '`',
        _ => return None,
    })
}

/// What an action does where it is bound, in words for the table and the
/// form.
fn action_label(mode: &str, action: Action) -> &'static str {
    match (mode, action) {
        ("reading", Action::Next) => "変換する",
        ("reading", Action::Previous) => "変換して最後の候補を選ぶ",
        ("reading", Action::Commit) => "かなのまま確定",
        ("reading", Action::Cancel) => "読みを取り消す",
        ("reading", Action::Register) => "この読みの語を登録する",
        ("candidates", Action::Cancel) => "読みに戻る",
        ("candidates", Action::Backspace) => "読みに戻って 1 文字消す",
        ("candidates", Action::Register) => "この読みの語を登録する",
        ("registration", Action::Commit) => "登録して確定",
        ("registration", Action::Cancel) => "登録をやめる",
        ("reading", Action::Begin) => "送り仮名を始める",
        ("candidates", Action::Begin) => "確定して次の読みを始める",
        (_, Action::Next) => "次の候補",
        (_, Action::Previous) => "前の候補",
        (_, Action::Commit) => "確定",
        (_, Action::Cancel) => "取り消す",
        (_, Action::Backspace) => "前の 1 文字を消す",
        (_, Action::Delete) => "後ろの 1 文字を消す",
        (_, Action::Left) => "カーソルを左へ",
        (_, Action::Right) => "カーソルを右へ",
        (_, Action::Home) => "カーソルを先頭へ",
        (_, Action::End) => "カーソルを末尾へ",
        (_, Action::Register) => "語を登録する",
        (_, Action::Forget) => "この候補を以後出さない",
        (_, Action::Form(Form::Hiragana)) => "ひらがなにする",
        (_, Action::Form(Form::Katakana)) => "カタカナにする",
        (_, Action::Form(Form::HalfKatakana)) => "半角カタカナにする",
        (_, Action::Form(Form::FullAlphanumeric)) => "全角英数にする",
        (_, Action::Form(Form::Alphanumeric)) => "半角英数にする",
        (_, Action::CommitForm(Form::Hiragana)) => "ひらがなで確定",
        (_, Action::CommitForm(Form::Katakana)) => "カタカナで確定",
        (_, Action::CommitForm(Form::HalfKatakana)) => "半角カタカナで確定",
        (_, Action::CommitForm(Form::FullAlphanumeric)) => "全角英数で確定",
        (_, Action::CommitForm(Form::Alphanumeric)) => "半角英数で確定",
        (_, Action::Abc) => "確定して ABC モードへ",
        (_, Action::Kana) => "かなモードへ",
        (_, Action::Begin) => "読みを始める",
        (_, Action::UndoCommit) => "直前の確定を取り消す",
        (_, Action::Pick(place)) => SELECT_LABELS
            .get(usize::from(place))
            .copied()
            .unwrap_or("候補を確定"),
    }
}

fn target_label(mode: &str, value: &str) -> String {
    match parse_action(value) {
        Some(action) => action_label(mode, action).to_owned(),
        None => shown(value),
    }
}

/// What a key can be set to in a mode, as (value in the file, label).
fn targets(mode: &str) -> Vec<(String, String)> {
    if mode == APPLICATION_TABLE {
        return sendable_keys()
            .into_iter()
            .map(|key| (key.to_owned(), shown(key)))
            .collect();
    }
    actions(mode)
        .into_iter()
        .map(|a| (format_action(a), action_label(mode, a).to_owned()))
        .collect()
}

/// Whether the IME said it lacks the Input Monitoring permission, the last
/// time a field had the focus. Read as the page is shown.
fn input_monitoring_missing() -> bool {
    #[cfg(target_os = "macos")]
    return kanaemi_config::input_monitoring_missing_file().is_some_and(|file| file.exists());
    #[cfg(not(target_os = "macos"))]
    return false;
}

/// Whether the IME said it lacks the Accessibility permission, read as
/// [`input_monitoring_missing`] is.
fn accessibility_missing() -> bool {
    #[cfg(target_os = "macos")]
    return kanaemi_config::accessibility_missing_file().is_some_and(|file| file.exists());
    #[cfg(not(target_os = "macos"))]
    return false;
}

#[component]
pub fn Keys() -> Element {
    let ctx = use_context::<Ctx>();
    let config = ctx
        .store
        .read()
        .state
        .as_ref()
        .map(|loaded| loaded.settings.config.clone())
        .unwrap_or_default();
    let shipped = Config::default();
    let current = config.bindings.clone();
    let mut filter = use_signal(String::new);
    let mut advanced = use_signal(|| false);
    let monitoring = (current.hold_a_key() && input_monitoring_missing()).then(|| {
        rsx! {
            div { class: "problems",
                p {
                    "押さえたままに割り当てたキーを使うには、macOS の「入力監視」で Kanaemi を許可します。一覧にないときは、一緒に開く Finder の Kanaemi.app を一覧にドラッグします。かなえみを新しくしたあとは、許可し直す必要があることがあります。"
                }
                div { class: "actions",
                    button { onclick: move |_| crate::open_input_monitoring(),
                        "入力監視の設定を開く"
                    }
                }
            }
        }
    });
    let accessibility = (current.sends_keys() && accessibility_missing()).then(|| {
        rsx! {
            div { class: "problems",
                p {
                    "キーを別のキーに置き換えて送ったり、直前の確定を取り消したりするには、macOS の「アクセシビリティ」で Kanaemi を許可します。一覧にないときは、一緒に開く Finder の Kanaemi.app を一覧にドラッグします。"
                }
                div { class: "actions",
                    button { onclick: move |_| crate::open_accessibility(),
                        "アクセシビリティの設定を開く"
                    }
                }
            }
        }
    });
    let switcher = rsx! {
        {monitoring}
        {accessibility}
        div { class: "view-switch", role: "tablist",
            button {
                class: if !advanced() { "selected" },
                role: "tab",
                onclick: move |_| advanced.set(false),
                "かんたん"
            }
            button {
                class: if advanced() { "selected" },
                role: "tab",
                onclick: move |_| advanced.set(true),
                "アドバンスド"
            }
        }
    };
    if !advanced() {
        return rsx! {
            {switcher}
            p { class: "lead",
                "やりたいことごとに、使うキーを決めます。キーは、そのことが働くどの場面でも同じように使えます。場面ごとに分けて決めたいときは「アドバンスド」で。キーをクリックすると変えられ、× で外せます。点線のキーは、一部の場面にだけ割り当ててあるもので、その場面を添えて示します。"
            }
            SimpleKeys { current }
        };
    }
    let modes = [
        (
            "reading",
            "読みを打っているとき",
            "かなで読みを打って、まだ変換していないとき（例：›かんじ）",
        ),
        (
            "candidates",
            "候補を選んでいるとき",
            "変換して、候補を選んでいるとき（例：»漢字）",
        ),
        (
            "registration",
            "語を登録しているとき",
            "辞書にない語を、新しく登録しているとき",
        ),
        (
            "kana",
            "かなモードで何も打っていないとき",
            "ここで ABC モードに切り替えるキーを決められます",
        ),
        (
            "abc",
            "ABC モードで何も打っていないとき",
            "ここでかなモードに切り替えるキーを決められます",
        ),
        (
            APPLICATION_TABLE,
            "ふだん",
            "変換中の文字がないとき。押したキーを別のキーとしてアプリに送ります（Ctrl+H で 1 文字消す、など）",
        ),
    ];
    rsx! {
        {switcher}
        p { class: "lead",
            "押したキーに機能を割り当てます。たとえば Space と Ctrl+N には「次の候補」が割り当ててあり、どちらでも次の候補に進めます。場面ごとに決められ、行をクリックすると変えられます。"
        }
        input {
            class: "filter",
            placeholder: "キーで絞り込む…",
            value: "{filter}",
            oninput: move |e| filter.set(e.value()),
        }
        for (mode , title , note) in modes {
            BindingSection {
                key: "{mode}",
                mode,
                title,
                note,
                bindings: bindings_table(&current, mode),
                defaults: bindings_table(&shipped.bindings, mode),
                filter: filter(),
            }
        }
    }
}

/// The rows of a section to show, those matching `query`. A binding the
/// user added and took out is gone; a default one taken out stays, so it can
/// go back to its default.
fn shown_lines(all: &[Line], query: &str) -> Vec<Line> {
    all.iter()
        .filter(|line| line.to.is_some() || line.default.is_some())
        .filter(|line| {
            query.is_empty()
                || line.from.contains(query)
                || line.to.as_deref().is_some_and(|to| to.contains(query))
        })
        .cloned()
        .collect()
}

/// One row of a section: the pressed key, what it is bound to now, and what
/// Kanaemi binds it to.
#[derive(Clone, Debug, PartialEq)]
struct Line {
    from: String,
    to: Option<String>,
    default: Option<String>,
}

#[component]
fn BindingSection(
    mode: &'static str,
    title: &'static str,
    note: &'static str,
    bindings: Vec<(String, String)>,
    defaults: Vec<(String, String)>,
    filter: String,
) -> Element {
    let ctx = use_context::<Ctx>();
    let mut editing = use_signal(|| None::<String>);
    let mut adding = use_signal(|| false);
    let mut froms: Vec<&String> = defaults.iter().map(|(f, _)| f).collect();
    for (from, _) in &bindings {
        if !froms.contains(&from) {
            froms.push(from);
        }
    }
    let value = |set: &[(String, String)], from: &str| {
        set.iter().find(|(f, _)| f == from).map(|(_, v)| v.clone())
    };
    let query = filter.to_lowercase();
    let all: Vec<Line> = froms
        .into_iter()
        .map(|from| Line {
            from: from.clone(),
            to: value(&bindings, from),
            default: value(&defaults, from),
        })
        .collect();
    let lines = shown_lines(&all, &query);
    if lines.is_empty() && !query.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { class: "group",
            h2 { "{title}" }
            p { class: "note", "{note}" }
            if mode == APPLICATION_TABLE {
                div { class: "section-actions",
                    button {
                        // Every binding of the scene, not only the ones the
                        // filter leaves in view.
                        disabled: all.iter().all(|l| l.to.is_none()),
                        onclick: {
                            let lines = all.clone();
                            move |_| {
                                for line in lines.iter().filter(|l| l.to.is_some()) {
                                    let value = line.default.is_some().then(|| UNBOUND.into());
                                    ctx.change(&["keys", mode, &line.from], value);
                                }
                            }
                        },
                        "すべて外す"
                    }
                    button {
                        disabled: all.iter().all(|l| l.to == l.default),
                        onclick: {
                            let lines = all.clone();
                            move |_| {
                                for line in lines.iter().filter(|l| l.to != l.default) {
                                    ctx.change(&["keys", mode, &line.from], None);
                                }
                            }
                        },
                        "すべて既定に戻す"
                    }
                }
            }
            div { class: "box",
                table { class: "bindings",
                    thead {
                        tr {
                            th { "押すキー" }
                            th { if mode == APPLICATION_TABLE { "送るキー" } else { "機能" } }
                            th {}
                        }
                    }
                    tbody {
                        for line in lines {
                            if editing().as_deref() == Some(line.from.as_str()) {
                                tr { key: "edit-{line.from}",
                                    td { colspan: "3",
                                        BindingForm {
                                            mode,
                                            line: Some(line.clone()),
                                            on_close: move |_| editing.set(None),
                                        }
                                    }
                                }
                            } else {
                                tr {
                                    key: "{line.from}",
                                    class: "binding-row",
                                    onclick: {
                                        let from = line.from.clone();
                                        move |_| {
                                            adding.set(false);
                                            editing.set(Some(from.clone()));
                                        }
                                    },
                                    td { class: "key-cell", span { class: "key-badge", "{shown(&line.from)}" } }
                                    td { class: "target",
                                        match line.to.as_deref() {
                                            Some(to) => target_label(mode, to),
                                            None => "（外してある）".to_owned(),
                                        }
                                    }
                                    td { class: "status",
                                        if line.to != line.default {
                                            button {
                                                class: "reset",
                                                onclick: {
                                                    let from = line.from.clone();
                                                    move |e: MouseEvent| {
                                                        e.stop_propagation();
                                                        ctx.change(&["keys", mode, &from], None);
                                                    }
                                                },
                                                match &line.default {
                                                    Some(default) => rsx! { "既定の「{target_label(mode, default)}」に戻す" },
                                                    None => rsx! { "外す" },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if adding() {
                    BindingForm { mode, line: None, on_close: move |_| adding.set(false) }
                } else {
                    button {
                        class: "add",
                        onclick: move |_| {
                            editing.set(None);
                            adding.set(true);
                        },
                        Icon { paths: icons::PLUS }
                        "割り当てを足す"
                    }
                }
            }
        }
    }
}

/// The key and how it is pressed: written with completion, or entered by
/// pressing it. Recording takes the key wherever the focus is in the field,
/// the button just pressed included.
#[component]
fn KeyField(chord: Signal<String>, press: Signal<Press>) -> Element {
    let mut key = chord;
    let mut recording = use_signal(|| false);
    // A modifier down with nothing else while recording, a tap if it is let
    // go so.
    let mut alone = use_signal(|| None::<&'static str>);
    rsx! {
        div {
            class: "key-field",
            onkeydown: move |e: KeyboardEvent| {
                if !recording() {
                    return;
                }
                e.prevent_default();
                e.stop_propagation();
                let mods = e.modifiers();
                let held = Held {
                    ctrl: mods.contains(Modifiers::CONTROL),
                    cmd: mods.contains(Modifiers::META),
                    alt: mods.contains(Modifiers::ALT),
                    shift: mods.contains(Modifiers::SHIFT),
                };
                let code = e.code().to_string();
                if let Some(modifier) = recorded_alone(&code) {
                    // Its own flag is the only one held when it is alone.
                    let flags = [held.ctrl, held.cmd, held.alt, held.shift];
                    let only = flags.iter().filter(|on| **on).count() <= 1;
                    alone.set(only.then_some(modifier));
                    return;
                }
                alone.set(None);
                if let Some(text) = recorded(held, &e.key().to_string(), &code) {
                    key.set(text);
                    press.set(Press::Down);
                    recording.set(false);
                }
            },
            onkeyup: move |e: KeyboardEvent| {
                if recording()
                    && let Some(modifier) = alone()
                    && recorded_alone(&e.code().to_string()) == Some(modifier)
                {
                    e.prevent_default();
                    key.set(modifier.to_owned());
                    press.set(Press::Tap);
                    alone.set(None);
                    recording.set(false);
                }
            },
            KeyInput {
                class: if recording() { "key recording" } else { "key" },
                placeholder: if recording() { "キーを押してください…" } else { "ctrl+k" },
                value: key(),
                readonly: recording(),
                // A way of pressing written out in the field is taken to the
                // choice beside it.
                oninput: move |v: String| {
                    let (typed, typed_press) = split_press(&v);
                    if typed_press != Press::Down {
                        press.set(typed_press);
                    }
                    key.set(typed.to_owned());
                },
                onkeydown: move |_| {},
            }
            select {
                class: "press",
                disabled: presses(key().trim()).len() < 2,
                onchange: move |e| {
                    if let Some(p) = Press::ALL.into_iter().find(|p| p.suffix() == e.value()) {
                        press.set(p);
                    }
                },
                for p in presses(key().trim()) {
                    option { value: "{p.suffix()}", selected: press() == p, "{p.label()}" }
                }
            }
            button {
                class: if recording() { "record recording" } else { "record" },
                title: "キーを押して入れる",
                "aria-label": "キーを押して入れる",
                onclick: move |_| {
                    if !recording() {
                        key.set(String::new());
                    }
                    recording.set(!recording());
                },
                if recording() {
                    "やめる"
                } else {
                    Icon { paths: icons::KEYBOARD }
                }
            }
        }
    }
}

/// The key in `key` pressed as `press` says, as the settings file writes
/// it; a way of pressing chosen for another key falls back to a press.
fn written_key(key: &str, press: Press) -> Option<String> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some(match press {
        p if presses(key).contains(&p) => p.written(key),
        _ => key.to_owned(),
    })
}

/// Adds a binding, or changes the one in `line`: the key, entered by
/// pressing it, and what it is bound to.
#[component]
fn BindingForm(mode: &'static str, line: Option<Line>, on_close: EventHandler<()>) -> Element {
    let ctx = use_context::<Ctx>();
    let original = line.clone();
    let (written, written_press) = line
        .as_ref()
        .map(|l| split_press(&l.from))
        .unwrap_or(("", Press::Down));
    let key = use_signal(|| written.to_owned());
    let press = use_signal(|| written_press);
    let targets = targets(mode);
    let mut target = use_signal(|| {
        line.as_ref()
            .and_then(|l| l.to.clone())
            .unwrap_or_else(|| targets[0].0.clone())
    });
    let mut error = use_signal(|| None::<String>);

    let save = {
        let original = original.clone();
        move |_| {
            let Some(from) = written_key(&key(), press()) else {
                return;
            };
            let mut changes = vec![(vec!["keys", mode, from.as_str()], Some(target().into()))];
            // A changed key leaves the old one as Kanaemi binds it.
            // The old key goes unless it is the new one written another way,
            // which writing the new one replaces.
            if let Some(old) = &original
                && old.from != from
                && parse_binding_key(&old.from) != parse_binding_key(&from)
            {
                changes.push((vec!["keys", mode, old.from.as_str()], None));
            }
            if let Err(message) = ctx.try_change_many(&changes) {
                error.set(Some(message));
                return;
            }
            on_close.call(());
        }
    };
    rsx! {
        div {
            class: "binding-form",
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    on_close.call(());
                }
            },
            div { class: "binding-form-row",
                KeyField { chord: key, press }
                span { class: "arrow", Icon { paths: icons::ARROW_RIGHT } }
                select {
                    value: "{target}",
                    onchange: move |e| target.set(e.value()),
                    for (value , label) in targets {
                        option { value: "{value}", selected: target() == value, "{label}" }
                    }
                }
            }
            if let Some(message) = error() {
                p { class: "error", "この割り当ては使えません：{message}" }
            }
            div { class: "binding-form-buttons",
                button { class: "primary", disabled: key().trim().is_empty(), onclick: save, if original.is_some() { "保存" } else { "足す" } }
                button { onclick: move |_| on_close.call(()), "キャンセル" }
                if let Some(old) = original.clone().filter(|l| l.to.is_some()) {
                    div { class: "spacer" }
                    button {
                        class: "danger",
                        onclick: move |_| {
                            // Kanaemi's own binding is taken out with @none;
                            // one the user added is just removed.
                            let value = old.default.is_some().then(|| UNBOUND.into());
                            ctx.change(&["keys", mode, &old.from], value);
                            on_close.call(());
                        },
                        "外す"
                    }
                }
            }
        }
    }
}

/// Where a scene is, in a few words for a key bound there only.
fn scene_label(scene: &str) -> &'static str {
    match scene {
        "kana" => "かなで何も打っていないとき",
        "abc" => "ABC で何も打っていないとき",
        "reading" => "読みを打っているとき",
        "candidates" => "候補を選んでいるとき",
        "registration" => "語を登録しているとき",
        _ => "",
    }
}

/// Writes every change, or none when one is refused.
fn apply(ctx: Ctx, changes: &[Change]) -> Result<(), String> {
    let changes: Vec<(Vec<&str>, Option<Value>)> = changes
        .iter()
        .map(|(scene, key, value)| {
            (
                vec!["keys", *scene, key.as_str()],
                value.clone().map(Into::into),
            )
        })
        .collect();
    ctx.try_change_many(&changes)
}

/// The keys by what they do, each set once for every scene it works in.
#[component]
fn SimpleKeys(current: Bindings) -> Element {
    let ctx = use_context::<Ctx>();
    let sending = intents::sending(&current);
    let mut error = use_signal(|| None::<String>);
    rsx! {
        for (title , list) in GROUPS.iter() {
            section { class: "group", key: "{title}",
                h2 { "{title}" }
                div { class: "box",
                    for intent in list.iter() {
                        IntentRow { key: "{intent.title}", intent: *intent, current: current.clone() }
                    }
                }
            }
        }
        section { class: "group",
            h2 { "ふだん" }
            div { class: "box",
                div { class: "row",
                    div { class: "row-main",
                        div { class: "row-text",
                            span { class: "label", "Emacs 風のキーを使う" }
                            span { class: "description",
                                "変換中の文字がないときも、Ctrl+H で 1 文字消す、Ctrl+A で行の先頭へ、などのキーをどのアプリでも使えるようにします。"
                            }
                        }
                        div { class: "control",
                            input {
                                class: "switch",
                                r#type: "checkbox",
                                role: "switch",
                                checked: sending,
                                onchange: {
                                    let current = current.clone();
                                    move |e: FormEvent| {
                                        let changes = intents::send(e.checked(), &current, &Bindings::default());
                                        error.set(apply(ctx, &changes).err());
                                    }
                                },
                            }
                        }
                    }
                    if let Some(message) = error() {
                        p { class: "error", "切り替えられませんでした：{message}" }
                    }
                }
            }
        }
    }
}

/// What the form under an intent does.
#[derive(Clone, Debug, PartialEq)]
enum Editing {
    Adding,
    /// Puts another key in place of this one.
    Changing(String),
}

/// One intent: its keys, each to change or take off, a way to add one, and
/// the way back to Kanaemi's own.
#[component]
fn IntentRow(intent: Intent, current: Bindings) -> Element {
    let ctx = use_context::<Ctx>();
    let shipped = Bindings::default();
    let keys = intent.keys(&current);
    let changed = intent.changed(&current, &shipped);
    let mut editing = use_signal(|| None::<Editing>);
    let mut key = use_signal(String::new);
    let mut press = use_signal(|| Press::Down);
    let mut error = use_signal(|| None::<String>);
    let mut write = move |changes: Vec<Change>| match apply(ctx, &changes) {
        Ok(()) => {
            error.set(None);
            true
        }
        Err(message) => {
            error.set(Some(message));
            false
        }
    };
    let remove = {
        let current = current.clone();
        move |key: String| {
            // A form changing the key taken off would have nothing left to
            // put the new key in place of.
            if write(intent.removing(&key, &current, &Bindings::default()))
                && editing() == Some(Editing::Changing(key))
            {
                editing.set(None);
            }
        }
    };
    let mut open = move |what: Editing| {
        let (written, written_press) = match &what {
            Editing::Adding => (String::new(), Press::Down),
            Editing::Changing(old) => {
                let (k, p) = split_press(old);
                (k.to_owned(), p)
            }
        };
        key.set(written);
        press.set(written_press);
        error.set(None);
        editing.set(Some(what));
    };
    let chips: Vec<(String, Option<String>)> = keys
        .everywhere
        .iter()
        .map(|k| (k.clone(), None))
        .chain(keys.somewhere.iter().map(|(k, scenes)| {
            let labels: Vec<&str> = scenes.iter().map(|s| scene_label(s)).collect();
            (k.clone(), Some(format!("{}だけ", labels.join("・"))))
        }))
        .collect();
    let empty = chips.is_empty();
    rsx! {
        div { class: "row intent",
            div { class: "row-main",
                div { class: "row-text",
                    span { class: "label", "{intent.title}" }
                    if !intent.note.is_empty() {
                        span { class: "description", "{intent.note}" }
                    }
                }
                div { class: "intent-keys",
                    for (k , place) in chips {
                        span {
                            class: if place.is_some() { "key-chip partial" } else { "key-chip" },
                            key: "{k}",
                            button {
                                class: "chip-key",
                                title: if place.is_some() { "変える（一部の場面だけの割り当て）" } else { "変える" },
                                onclick: {
                                    let k = k.clone();
                                    move |_| open(Editing::Changing(k.clone()))
                                },
                                "{shown(&k)}"
                                if let Some(place) = place.clone() {
                                    span { class: "chip-where", "{place}" }
                                }
                            }
                            button {
                                class: "chip-remove",
                                title: "外す",
                                "aria-label": "{shown(&k)} を外す",
                                onclick: {
                                    let mut remove = remove.clone();
                                    let k = k.clone();
                                    move |_| remove(k.clone())
                                },
                                Icon { paths: icons::X }
                            }
                        }
                    }
                    if empty {
                        span { class: "none", "割り当てなし" }
                    }
                    if editing().is_none() {
                        button {
                            class: "chip-add",
                            title: "キーを足す",
                            "aria-label": "キーを足す",
                            onclick: move |_| open(Editing::Adding),
                            Icon { paths: icons::PLUS }
                        }
                    }
                }
            }
            if let Some(what) = editing() {
                div {
                    class: "binding-form",
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Escape {
                            editing.set(None);
                        }
                    },
                    div { class: "binding-form-row", KeyField { chord: key, press } }
                    div { class: "binding-form-buttons",
                        button {
                            class: "primary",
                            disabled: key().trim().is_empty(),
                            onclick: {
                                let current = current.clone();
                                let what = what.clone();
                                move |_| {
                                    let Some(written) = written_key(&key(), press()) else {
                                        return;
                                    };
                                    let shipped = Bindings::default();
                                    let changes = match &what {
                                        Editing::Adding => intent.adding(&written, &shipped),
                                        Editing::Changing(old) => {
                                            intent.replacing(old, &written, &current, &shipped)
                                        }
                                    };
                                    if write(changes) {
                                        editing.set(None);
                                    }
                                }
                            },
                            match what {
                                Editing::Adding => "足す",
                                Editing::Changing(_) => "保存",
                            }
                        }
                        button { onclick: move |_| editing.set(None), "キャンセル" }
                    }
                }
            }
            if let Some(message) = error() {
                p { class: "error", "この割り当ては使えません：{message}" }
            }
            if changed {
                div { class: "reset-line",
                    button {
                        class: "reset",
                        onclick: {
                            let current = current.clone();
                            move |_| {
                                write(intent.resetting(&current, &Bindings::default()));
                            }
                        },
                        "既定に戻す"
                    }
                }
            }
        }
    }
}

/// A key as written in the settings file, the way keyboards label it:
/// `ctrl+h` is shown as `Ctrl+H`.
pub fn shown(text: &str) -> String {
    if text.starts_with('@') {
        return text.to_owned();
    }
    if let Some(key) = text.strip_suffix("#tap").filter(|k| !k.is_empty()) {
        return format!("{} の単独押し", shown(key));
    }
    if let Some(key) = text.strip_suffix("#hold").filter(|k| !k.is_empty()) {
        return format!("{} を押さえたまま", shown(key));
    }
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some((modifier, key)) = rest.split_once('+')
        && !key.is_empty()
    {
        parts.push(
            match modifier {
                "ctrl" => "Ctrl",
                "cmd" => CMD,
                "alt" => ALT,
                "shift" => "Shift",
                other => other,
            }
            .to_owned(),
        );
        rest = key;
    }
    parts.push(match rest {
        "space" => "Space".to_owned(),
        "enter" => "Enter".to_owned(),
        "esc" => "Esc".to_owned(),
        "backspace" => "Backspace".to_owned(),
        "delete" => "Delete".to_owned(),
        "left" => "←".to_owned(),
        "right" => "→".to_owned(),
        "up" => "↑".to_owned(),
        "down" => "↓".to_owned(),
        "home" => "Home".to_owned(),
        "end" => "End".to_owned(),
        "left-shift" => "左 Shift".to_owned(),
        "right-shift" => "右 Shift".to_owned(),
        "left-ctrl" => "左 Ctrl".to_owned(),
        "right-ctrl" => "右 Ctrl".to_owned(),
        "left-cmd" => format!("左 {CMD}"),
        "right-cmd" => format!("右 {CMD}"),
        "left-alt" => format!("左 {ALT}"),
        "right-alt" => format!("右 {ALT}"),
        "eisu" => "英数".to_owned(),
        "kana" => "かな".to_owned(),
        "henkan" => "変換".to_owned(),
        "muhenkan" => "無変換".to_owned(),
        key => key.to_uppercase(),
    });
    parts.join("+")
}

#[cfg(target_os = "macos")]
const CMD: &str = "Cmd";
#[cfg(not(target_os = "macos"))]
const CMD: &str = "Win";
#[cfg(target_os = "macos")]
const ALT: &str = "Option";
#[cfg(not(target_os = "macos"))]
const ALT: &str = "Alt";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_split_from_how_it_is_pressed() {
        assert_eq!(split_press("left-shift#tap"), ("left-shift", Press::Tap));
        assert_eq!(split_press("space#hold"), ("space", Press::Hold));
        assert_eq!(split_press(";#hold"), (";", Press::Hold));
        assert_eq!(split_press("ctrl+h"), ("ctrl+h", Press::Down));
        assert_eq!(split_press("#"), ("#", Press::Down));
    }

    #[test]
    fn a_key_offers_only_the_ways_it_can_be_pressed() {
        assert_eq!(presses("left-shift"), [Press::Down, Press::Tap]);
        assert_eq!(presses(";"), [Press::Down, Press::Hold]);
        assert_eq!(presses("space"), [Press::Down, Press::Hold]);
        assert_eq!(presses("enter"), [Press::Down]);
        assert_eq!(presses("ctrl+h"), [Press::Down]);
    }

    #[test]
    fn a_key_and_how_it_is_pressed_are_written_together() {
        assert_eq!(Press::Tap.written("left-shift"), "left-shift#tap");
        assert_eq!(Press::Hold.written(";"), ";#hold");
        assert_eq!(Press::Down.written("ctrl+h"), "ctrl+h");
    }

    #[test]
    fn a_modifier_pressed_alone_is_recorded_as_its_side() {
        assert_eq!(recorded_alone("ShiftLeft"), Some("left-shift"));
        assert_eq!(recorded_alone("MetaRight"), Some("right-cmd"));
        assert_eq!(recorded_alone("AltLeft"), Some("left-alt"));
        assert_eq!(recorded_alone("ControlRight"), Some("right-ctrl"));
        assert_eq!(recorded_alone("KeyA"), None);
    }

    fn held(ctrl: bool, cmd: bool, alt: bool, shift: bool) -> Held {
        Held {
            ctrl,
            cmd,
            alt,
            shift,
        }
    }

    #[test]
    fn keys_are_written_the_way_the_settings_file_reads_them() {
        let none = Held::default();
        assert_eq!(
            recorded(held(true, false, false, false), "h", "KeyH").as_deref(),
            Some("ctrl+h")
        );
        assert_eq!(
            recorded(held(false, false, false, true), " ", "Space").as_deref(),
            Some("shift+space")
        );
        assert_eq!(
            recorded(held(true, true, true, true), "ArrowLeft", "ArrowLeft").as_deref(),
            Some("ctrl+cmd+alt+shift+left")
        );
        assert_eq!(recorded(none, "F12", "F12").as_deref(), Some("f12"));
        assert_eq!(recorded(none, "0", "Digit0").as_deref(), Some("0"));
        assert_eq!(recorded(none, "Escape", "Escape").as_deref(), Some("esc"));
    }

    #[test]
    fn a_letter_is_what_it_types_on_any_layout() {
        assert_eq!(recorded(Held::default(), "a", "KeyQ").as_deref(), Some("a"));
        assert_eq!(
            recorded(held(true, false, false, false), "a", "KeyQ").as_deref(),
            Some("ctrl+a")
        );
    }

    #[test]
    fn option_with_a_digit_or_symbol_is_the_key_not_what_it_types() {
        let alt = held(false, false, true, false);
        assert_eq!(
            recorded(alt, "\u{2122}", "Digit2").as_deref(),
            Some("alt+2")
        );
        assert_eq!(
            recorded(alt, "\u{2026}", "Semicolon").as_deref(),
            Some("alt+;")
        );
    }

    #[test]
    fn option_and_shift_with_a_digit_is_the_shifted_symbol() {
        let both = held(false, false, true, true);
        assert_eq!(
            recorded(both, "\u{20ac}", "Digit2").as_deref(),
            Some("alt+@")
        );
    }

    #[test]
    fn a_capital_typed_without_shift_stays_a_capital() {
        assert_eq!(recorded(Held::default(), "A", "KeyA").as_deref(), Some("A"));
    }

    #[test]
    fn a_letter_is_named_by_its_place_on_the_keyboard() {
        assert_eq!(
            recorded(held(false, false, true, false), "ƒ", "KeyF").as_deref(),
            Some("alt+f")
        );
        assert_eq!(
            recorded(held(false, false, false, true), "A", "KeyA").as_deref(),
            Some("shift+a")
        );
    }

    #[test]
    fn a_shifted_symbol_is_the_symbol_it_types() {
        assert_eq!(
            recorded(held(false, false, false, true), ":", "Semicolon").as_deref(),
            Some(":")
        );
    }

    #[test]
    fn keys_are_shown_the_way_keyboards_label_them() {
        assert_eq!(shown("ctrl+h"), "Ctrl+H");
        assert_eq!(shown("shift+space"), "Shift+Space");
        assert_eq!(shown("ctrl++"), "Ctrl++");
        assert_eq!(shown("ctrl+left"), "Ctrl+←");
        assert_eq!(shown("right-shift"), "右 Shift");
        assert_eq!(shown("right-shift#tap"), "右 Shift の単独押し");
        assert_eq!(shown("space#hold"), "Space を押さえたまま");
        assert_eq!(shown("#"), "#");
        assert_eq!(shown("f12"), "F12");
        assert_eq!(shown(";"), ";");
        assert_eq!(shown("@register"), "@register");
    }

    fn line(from: &str, to: Option<&str>, default: Option<&str>) -> Line {
        Line {
            from: from.to_owned(),
            to: to.map(str::to_owned),
            default: default.map(str::to_owned),
        }
    }

    #[test]
    fn a_default_binding_taken_out_stays_listed_so_it_can_go_back() {
        let all = [
            line("ctrl+n", None, Some("@next")),
            line("space", Some("@next"), Some("@next")),
        ];

        let shown = shown_lines(&all, "");

        assert_eq!(shown, all);
    }

    #[test]
    fn a_binding_the_user_added_and_took_out_is_gone() {
        let all = [line("ctrl+x", None, None)];

        assert_eq!(shown_lines(&all, ""), []);
    }

    #[test]
    fn the_filter_matches_the_key_or_what_it_is_bound_to() {
        let all = [
            line("ctrl+n", Some("@next"), Some("@next")),
            line("enter", Some("@commit"), Some("@commit")),
        ];

        assert_eq!(shown_lines(&all, "comm"), [all[1].clone()]);
        assert_eq!(shown_lines(&all, "ctrl"), [all[0].clone()]);
    }

    #[test]
    fn the_japanese_keys_are_named_in_japanese_and_recorded() {
        assert_eq!(shown("henkan"), "変換");
        assert_eq!(
            recorded(Held::default(), "NonConvert", "NonConvert").as_deref(),
            Some("muhenkan")
        );
        assert_eq!(shown("eisu"), "英数");
        assert_eq!(shown("kana"), "かな");
        assert_eq!(
            recorded(Held::default(), "Lang1", "Lang1").as_deref(),
            Some("kana")
        );
        assert_eq!(
            recorded(Held::default(), "Lang2", "Lang2").as_deref(),
            Some("eisu")
        );
    }

    #[test]
    fn begin_is_offered_where_it_works_and_described_by_what_it_does_there() {
        let label = |mode| {
            targets(mode)
                .into_iter()
                .find(|(value, _)| value == "@begin")
                .map(|(_, label)| label)
        };
        assert_eq!(label("kana").as_deref(), Some("読みを始める"));
        assert_eq!(label("reading").as_deref(), Some("送り仮名を始める"));
        assert_eq!(
            label("candidates").as_deref(),
            Some("確定して次の読みを始める")
        );
        assert_eq!(label("registration").as_deref(), Some("読みを始める"));
        assert_eq!(label("abc"), None);
    }

    #[test]
    fn a_modifier_alone_is_not_a_key() {
        assert_eq!(
            recorded(held(true, false, false, false), "Control", "ControlLeft"),
            None
        );
        assert_eq!(recorded(Held::default(), "Unidentified", ""), None);
    }
}
