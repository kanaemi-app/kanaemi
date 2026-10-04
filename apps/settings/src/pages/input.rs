use super::*;

/// The text of a romaji table: a bundled one by name, or a file in the
/// romaji folder.
fn romaji_text(dir: &Path, name: &str) -> Option<String> {
    read_romaji_table(dir, name).ok()
}

/// The signs printed on Cmd, Ctrl and Option keys; other keyboards print
/// the names.
#[cfg(target_os = "macos")]
const MODIFIER_ICONS: [&str; 3] = ["⌘", "⌃", "⌥"];
#[cfg(not(target_os = "macos"))]
const MODIFIER_ICONS: [&str; 3] = ["Win", "Ctrl", "Alt"];

#[component]
pub fn Input() -> Element {
    let ctx = use_context::<Ctx>();
    let config = config(ctx);
    let shipped = Config::default();
    let dir = ctx.store.read().dir.clone();
    let romaji_written = ctx
        .store
        .read()
        .state
        .as_ref()
        .ok()
        .and_then(|l| l.settings.romaji_tables.clone());

    let names = bundled_romaji_tables()
        .map(str::to_owned)
        .chain(files_with(&dir.join(ROMAJI_DIR), &[TEXT_EXTENSION]));
    let tables: Vec<ListItem> = names
        .map(|name| {
            let text = romaji_text(&dir, &name);
            ListItem {
                description: text.as_deref().and_then(description),
                warning: text.as_deref().and_then(romaji_warning),
                label: name.clone(),
                name,
                meta: None,
                convert: None,
            }
        })
        .collect();
    let mut looking = use_signal(|| None::<String>);
    let default_tables: Vec<String> = default_romaji_tables()
        .iter()
        .map(|n| (*n).to_owned())
        .collect();
    let chosen_tables = romaji_written.unwrap_or_else(|| default_tables.clone());

    let control_port = ctx
        .store
        .read()
        .state
        .as_ref()
        .ok()
        .and_then(|l| l.settings.control_port);
    let pass = config.pass_while_composing;
    let pass_names = [
        (
            "cmd",
            MODIFIER_ICONS[0],
            "Cmd",
            "Cmd+C・Cmd+V などのアプリのショートカット",
            pass.cmd,
        ),
        (
            "ctrl",
            MODIFIER_ICONS[1],
            "Ctrl",
            "Ctrl を使うアプリのショートカット",
            pass.ctrl,
        ),
        (
            "alt",
            MODIFIER_ICONS[2],
            "Option",
            "Option で打つ記号やショートカット",
            pass.alt,
        ),
    ];
    let write_pass = move |name: &'static str, on: bool| {
        let names: Vec<&str> = pass_names
            .iter()
            .filter(|(n, _, _, _, was)| if *n == name { on } else { *was })
            .map(|(n, _, _, _, _)| *n)
            .collect();
        ctx.change(
            &["keys", "pass_while_composing"],
            Some(names.into_iter().collect()),
        );
    };

    rsx! {
        Group {
            title: "ローマ字の表",
            note: "打ったローマ字をかなに変える表です。上から順に重ね、同じ綴りは下の表が優先されます。左のつまみをドラッグして順番を変え、スイッチで使うかどうかを決めます。自分で作った表は romaji フォルダに置くとここに出ます。",
            footer: rsx! {
                button {
                    onclick: {
                        let folder = dir.join(ROMAJI_DIR);
                        move |_| open_folder(&folder)
                    },
                    Icon { paths: icons::FOLDER_OPEN }
                    "romaji フォルダを開く"
                }
                ResetLine {
                    shipped: (chosen_tables != default_tables).then(|| default_tables.join("・")),
                    path: path(&["romaji", "tables"]),
                }
            },
            OrderedList {
                items: tables,
                chosen: chosen_tables,
                fixed: Vec::new(),
                path: path(&["romaji", "tables"]),
                on_info: move |name: String| looking.set(Some(name)),
            }
        }
        if let Some(name) = looking() {
            RomajiRules {
                text: romaji_text(&dir, &name).unwrap_or_default(),
                name,
                on_close: move |_| looking.set(None),
            }
        }
        Group {
            title: "変換中のショートカット",
            note: "変換中の文字があるときに、選んだキーとの組み合わせ（Cmd+C など）は、文字を確定してからアプリに送ります。選んでいないものは、変換中の文字をうっかり失わないよう、押しても何もしません。",
            footer: rsx! {
                span {}
                ResetLine {
                    shipped: (pass != shipped.pass_while_composing).then(|| "どれも使わない".to_owned()),
                    path: path(&["keys", "pass_while_composing"]),
                }
            },
            for (name , icon , label , what , on) in pass_names {
                    KeyToggle {
                        key: "{name}",
                        symbol: icon,
                        name: label,
                        what,
                        on,
                        onclick: move |_| write_pass(name, !on),
                    }
                }
        }
        Group { title: "単独押し",
            Row {
                label: "単独押しとみなす長さ",
                description: "修飾キーをこの長さ（ミリ秒）のうちに離すと単独押しです。長く押さえたときは何もしません。押さえたままの割り当てのあるキーも、この長さで、押さえたままか単独で押したかを見分けます。",
                path: path(&["keys", "tap_timeout_ms"]),
                shipped: (config.tap_timeout_ms != shipped.tap_timeout_ms)
                    .then(|| format!("{} ミリ秒", shipped.tap_timeout_ms)),
                input {
                    class: "number",
                    r#type: "number",
                    min: "1",
                    value: "{config.tap_timeout_ms}",
                    onchange: move |e| {
                        let value = match e.value().parse::<i64>() {
                            Ok(ms) => ms.into(),
                            Err(_) => e.value().into(),
                        };
                        ctx.change(&["keys", "tap_timeout_ms"], Some(value));
                    },
                }
            }
        }
        Group {
            title: "外からの操作",
            note: "エディタなどのほかのプログラムが、このポート（127.0.0.1）につないで、入力モードを知り、変えられます。つないできた相手は確かめないので、同じマシンのどのプログラムからでもつなげます。空にすると待ちません。",
            Row {
                label: "待つポート",
                description: "1 から 65535 までの、ほかで使っていない番号を選んでください。".to_owned(),
                path: path(&["control", "port"]),
                shipped: control_port.is_some().then(|| "待たない".to_owned()),
                input {
                    class: "number",
                    r#type: "number",
                    min: "1",
                    max: "65535",
                    placeholder: "50123",
                    value: control_port.map(|p| p.to_string()).unwrap_or_default(),
                    onchange: move |e| {
                        let text = e.value();
                        let value = match text.trim() {
                            "" => None,
                            port => Some(match port.parse::<i64>() {
                                Ok(port) => port.into(),
                                Err(_) => text.clone().into(),
                            }),
                        };
                        ctx.change(&["control", "port"], value);
                    },
                }
            }
        }
    }
}

/// Every rule of a romaji table, to see what typing does before choosing it.
#[component]
fn RomajiRules(name: String, text: String, on_close: EventHandler<()>) -> Element {
    let mut filter = use_signal(String::new);
    let query = filter();
    let rules: Vec<(String, String)> = text
        .trim_start_matches('\u{feff}')
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| match line.strip_prefix('!') {
            Some(input) => (input.to_owned(), "（ほかの表の規則を消す）".to_owned()),
            None => {
                let (input, output) = line.split_once('\t').unwrap_or((line, ""));
                (input.to_owned(), output.to_owned())
            }
        })
        .filter(|(input, output)| {
            query.is_empty() || input.contains(&query) || output.contains(&query)
        })
        .collect();
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "modal",
                onclick: move |e| e.stop_propagation(),
                header {
                    div {
                        h2 { "{name}" }
                        if let Some(description) = description(&text) {
                            p { class: "item-description", "{description}" }
                        }
                    }
                    button { onclick: move |_| on_close.call(()), "閉じる" }
                }
                input {
                    class: "filter",
                    placeholder: "ローマ字やかなで絞り込む…",
                    value: "{filter}",
                    oninput: move |e| filter.set(e.value()),
                }
                div { class: "modal-body",
                    table { class: "rules",
                        thead {
                            tr {
                                th { "打つ" }
                                th { "出る" }
                            }
                        }
                        tbody {
                            for (input , output) in rules {
                                tr {
                                    td { span { class: "key-badge", "{input}" } }
                                    td { "{output}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What to say about the lines of a romaji table the IME skips, or `None`
/// when every line is read. The table is still used without them.
fn romaji_warning(text: &str) -> Option<String> {
    let invalid = RomajiTable::empty().apply(text);
    if invalid.is_empty() {
        return None;
    }
    let lines: Vec<String> = invalid.iter().map(usize::to_string).collect();
    Some(format!(
        "{} 行目を読めません。ほかの行は使います",
        lines.join("・")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_romaji_table_whose_lines_all_read_has_nothing_to_say() {
        assert_eq!(romaji_warning("# 説明\nka\tか\n"), None);
    }

    #[test]
    fn the_invalid_lines_of_a_romaji_table_are_named_by_number() {
        let said = romaji_warning("# 説明\nka\tか\nbad line\nki\tき\na b\tあ\n");

        assert_eq!(
            said.as_deref(),
            Some("3・5 行目を読めません。ほかの行は使います")
        );
    }
}
