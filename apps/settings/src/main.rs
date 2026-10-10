//! The settings app.
//!
//! A program of its own rather than a window of the input method, so the
//! input method stays a background process that only handles keys. It reads
//! and writes the files of the settings folder and never talks to the input
//! method, which reads the files again when a field next takes the focus.
//! On Windows the input method also runs inside other applications'
//! processes, where it cannot open windows of its own.

// No console window opens beside the app on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;

use dioxus::desktop::tao::event::Event as WryEvent;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder, WindowEvent, use_wry_event_handler};
use dioxus::prelude::*;
use kanaemi_config::Value;

mod apps;
mod cache;
mod checks;
mod complete;
mod controls;
mod convert;
mod highlight;
mod icons;
mod intents;
mod keys;
mod logs;
mod messages;
mod official;
mod pages;
mod reorder;
mod send_except;
mod store;

use icons::Icon;
use store::Store;

const STYLE: &str = include_str!("../assets/style.css");

fn main() {
    // How the install scripts learn the version to write into the bundle and
    // the IBus component.
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("{}", kanaemi_core::VERSION);
        return;
    }
    let window = WindowBuilder::new()
        .with_title("かなえみ設定")
        // Wide enough that a key binding row keeps its keys beside what they
        // do, and short enough for a laptop screen of 900 points.
        .with_inner_size(LogicalSize::new(1080.0, 780.0))
        .with_min_inner_size(LogicalSize::new(720.0, 480.0));
    let config = Config::new().with_window(window).with_menu(None);
    // The logo build.rs compiled in, which the shell shows for the program;
    // Dioxus's own icon otherwise.
    #[cfg(windows)]
    let config = {
        use dioxus::desktop::tao::platform::windows::IconExtWindows;
        use dioxus::desktop::tao::window::Icon;
        match Icon::from_resource(1, None) {
            Ok(icon) => config.with_icon(icon),
            Err(_) => config,
        }
    };
    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(App);
}

/// What every page reads and changes.
#[derive(Clone, Copy)]
pub struct Ctx {
    pub store: Signal<Store>,
    /// Why the last change of an item, by its path, was refused.
    pub errors: Signal<HashMap<String, String>>,
}

impl Ctx {
    /// Writes `value` at `path`, or removes it with `None`; a refusal is kept
    /// to be shown beside the item.
    pub fn change(self, path: &[&str], value: Option<Value>) {
        let result = self.try_change(path, value);
        self.keep_refusal(&path.join("."), result);
    }

    fn keep_refusal(mut self, item: &str, result: Result<(), String>) {
        let item = item.to_owned();
        match result {
            Ok(()) => {
                self.errors.write().remove(&item);
            }
            Err(error) => {
                self.errors.write().insert(item, error);
            }
        }
    }

    /// Writes `value` at `path`, or removes it with `None`, leaving the
    /// refusal to the caller.
    pub fn try_change(mut self, path: &[&str], value: Option<Value>) -> Result<(), String> {
        self.store.write().change(path, value)
    }

    pub fn try_change_many(mut self, changes: &[(Vec<&str>, Option<Value>)]) -> Result<(), String> {
        self.store.write().change_many(changes)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Display,
    Input,
    Dictionaries,
    Functions,
    Keys,
    About,
}

impl Page {
    const ALL: [Page; 6] = [
        Page::Display,
        Page::Input,
        Page::Dictionaries,
        Page::Functions,
        Page::Keys,
        Page::About,
    ];

    fn title(self) -> &'static str {
        match self {
            Page::Display => "表示",
            Page::Input => "入力",
            Page::Dictionaries => "辞書",
            Page::Functions => "関数",
            Page::Keys => "キーバインド",
            Page::About => "かなえみについて",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Page::Display => icons::EYE,
            Page::Input => icons::KEYBOARD,
            Page::Dictionaries => icons::BOOK,
            Page::Functions => icons::CODE,
            Page::Keys => icons::COMMAND,
            Page::About => icons::INFO_CIRCLE,
        }
    }
}

#[component]
fn App() -> Element {
    let Some(dir) = kanaemi_config::dir() else {
        return rsx! {
            style { {STYLE} }
            main { class: "broken", p { "設定のフォルダが分かりません。" } }
        };
    };
    let mut store = use_signal(|| Store::open(dir));
    let errors = use_signal(HashMap::new);
    // Coming back to the window, read again what may have changed meanwhile:
    // the settings file edited by hand, tables and dictionaries put in their
    // folders.
    use_wry_event_handler(move |event, _| {
        if let WryEvent::WindowEvent {
            event: WindowEvent::Focused(true),
            ..
        } = event
        {
            let dir = store.peek().dir.clone();
            store.set(Store::open(dir));
        }
    });
    use_context_provider(|| Ctx { store, errors });
    let mut page = use_signal(|| Page::Display);

    let state = store.read();
    let body = match &state.state {
        Err(message) => rsx! {
            Broken { dir: state.dir.display().to_string(), message: message.clone() }
        },
        Ok(loaded) => rsx! {
            if loaded.problems.iter().any(|p| store::refuses(&p.kind)) {
                div { class: "problems",
                    p { "設定ファイルに、使えない設定があります。その設定は既定のまま動きます。" }
                    ul {
                        // A romaji table's invalid lines show beside the table.
                        for problem in loaded.problems.iter().filter(|p| store::refuses(&p.kind)) {
                            li { "{messages::describe(&problem.kind)}" span { class: "item", "（{problem.item}）" } }
                        }
                    }
                }
            }
            match page() {
                Page::Display => rsx! { pages::Display {} },
                Page::Input => rsx! { pages::Input {} },
                Page::Dictionaries => rsx! { pages::Dictionaries {} },
                Page::Functions => rsx! { pages::Functions {} },
                Page::Keys => rsx! { keys::Keys {} },
                Page::About => rsx! { pages::About {} },
            }
        },
    };
    rsx! {
        style { {STYLE} }
        div { class: "app", lang: "ja",
            nav {
                for p in Page::ALL {
                    button {
                        class: if page() == p { "selected" },
                        onclick: move |_| page.set(p),
                        Icon { paths: p.icon() }
                        span { "{p.title()}" }
                    }
                }
            }
            main {
                header { class: "title", "{page().title()}" }
                div { class: "content", {body} }
            }
        }
    }
}

/// A file that is not TOML is left for the user to fix by hand: editing it
/// would mean guessing what was meant.
#[component]
fn Broken(dir: String, message: String) -> Element {
    let ctx = use_context::<Ctx>();
    rsx! {
        div { class: "problems",
            p { "設定ファイルの書き方に誤りがあるため、ここでは変更できません。ファイルを直してから「読み直す」を押してください。" }
            pre { "{message}" }
            div { class: "actions",
                button { onclick: move |_| open_folder(Path::new(&dir)), Icon { paths: icons::FOLDER_OPEN } "フォルダを開く" }
                button {
                    onclick: move |_| {
                        let mut store = ctx.store;
                        let dir = store.read().dir.clone();
                        store.set(Store::open(dir));
                    },
                    "読み直す"
                }
            }
        }
    }
}

/// Shows a folder in Finder, or in Explorer on Windows.
pub fn open_folder(path: &Path) {
    let _ = std::fs::create_dir_all(path);
    open(path.as_os_str());
}

/// Opens a page in the default browser.
pub fn open_url(url: &str) {
    open(OsStr::new(url));
}

/// Opens the Input Monitoring settings, with Kanaemi.app selected in Finder
/// beside them to drag in: an input method is not listed there until it is
/// added by hand.
pub fn open_input_monitoring() {
    open(OsStr::new(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
    ));
    reveal_bundle();
}

/// Opens the Accessibility settings, with Kanaemi.app beside them as for
/// Input Monitoring.
pub fn open_accessibility() {
    open(OsStr::new(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
    ));
    reveal_bundle();
}

/// Selects Kanaemi.app in Finder, to drag into a permission's list.
fn reveal_bundle() {
    // This app ships inside Kanaemi.app.
    let bundle = std::env::current_exe().ok().and_then(|exe| {
        exe.ancestors()
            .find(|dir| dir.file_name() == Some(OsStr::new("Kanaemi.app")))
            .map(Path::to_path_buf)
    });
    if let Some(bundle) = bundle {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(bundle)
            .spawn();
    }
}

fn open(target: &OsStr) {
    #[cfg(windows)]
    let opener = "explorer";
    #[cfg(not(windows))]
    let opener = "open";
    let _ = std::process::Command::new(opener).arg(target).spawn();
}
