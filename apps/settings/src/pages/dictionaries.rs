use super::*;

#[component]
pub fn Dictionaries() -> Element {
    let ctx = use_context::<Ctx>();
    let store = ctx.store.read();
    let store_dir = store.dir.clone();
    let folder = store.dir.join(DICTIONARY_DIR);
    let written = store
        .state
        .as_ref()
        .ok()
        .and_then(|l| l.settings.dictionaries.clone());
    drop(store);
    let files = dictionary_files(&folder);
    let chosen: Vec<String> = match &written {
        Some(sources) => sources.iter().map(|s| source_name(&folder, s)).collect(),
        None => default_dictionaries(&store_dir),
    };
    let custom = store_dir.join(USER_CUSTOM_FILE);
    let custom_path = custom.clone();
    let mut items = vec![ListItem {
        name: USER_CUSTOM.to_owned(),
        label: "ユーザー辞書".to_owned(),
        description: Some("あなたが登録した語。いつも使い、外せません。".to_owned()),
        meta: file_size(&custom),
        convert: None,
        warning: invalid_lines(&custom, true),
        chip: None,
    }];
    items.extend(files.iter().map(|name| {
        let path = folder.join(name);
        let convertible = !is_binary(&path) && binary_name(name) != *name;
        let status = convertible.then(|| conversion(&folder, name));
        let (meta, convert) = match status {
            None => (file_size(&path), None),
            Some(Conversion::None) => (file_size(&path), Some("バイナリに変換")),
            Some(Conversion::Stale) => (
                file_size(&path).map(|size| format!("要再変換・{size}")),
                Some("バイナリに再変換"),
            ),
            Some(Conversion::Current) => (
                file_size(&path).map(|size| format!("変換済み・{size}")),
                None,
            ),
        };
        ListItem {
            name: name.clone(),
            label: name.clone(),
            description: file_description(&path),
            meta,
            convert: convert.map(str::to_owned),
            warning: (!is_binary(&path))
                .then(|| invalid_lines(&path, false))
                .flatten(),
            chip: None,
        }
    }));
    items.extend(BUILTIN_DICTIONARIES.iter().map(|builtin| ListItem {
        name: format!("{BUILTIN_PREFIX}{}", builtin.name),
        label: description(builtin.text).unwrap_or_else(|| builtin.name.to_owned()),
        description: Some("かなえみに入っている辞書。変換するたびに値が決まる語です。".to_owned()),
        meta: None,
        convert: None,
        warning: None,
        chip: Some("組み込み".to_owned()),
    }));
    // A listed file that is gone still shows, so it can be taken out.
    for name in &chosen {
        if !items.iter().any(|i| &i.name == name) {
            items.push(ListItem {
                name: name.clone(),
                label: name.clone(),
                description: Some("ファイルが見つかりません".to_owned()),
                meta: None,
                convert: None,
                warning: None,
                chip: None,
            });
        }
    }
    let chosen_for_official = chosen.clone();
    let mut looking = use_signal(|| None::<String>);
    let looked = looking().map(|name| {
        let builtin = name
            .strip_prefix(BUILTIN_PREFIX)
            .and_then(builtin_dictionary);
        match builtin {
            _ if name == USER_CUSTOM => (
                "ユーザー辞書".to_owned(),
                custom.clone(),
                Lookup::UserCustom,
            ),
            Some(builtin) => (
                description(builtin.text).unwrap_or_else(|| name.clone()),
                PathBuf::from(&name),
                Lookup::Builtin(builtin.text),
            ),
            None => (name.clone(), folder.join(&name), Lookup::File),
        }
    });
    let on_convert = {
        let folder = folder.clone();
        let listed = written.is_some().then(|| chosen.clone());
        move |name: String| convert_and_use(ctx, &folder, listed.as_deref(), &name)
    };
    rsx! {
        Group {
            title: "使う辞書",
            note: "変換の候補を探す辞書です。上の辞書の候補ほど先に出ます。左のつまみをドラッグして順番を変え、スイッチで使うかどうかを決めます。辞書のファイル（.tsv と .kdic）は dictionaries フォルダ（中のフォルダも）に置くとここに出ます。SKK の辞書は取り込むと使えます。大きなテキストの辞書は「バイナリに変換」すると速く開けます。同じ名前の .kdic があれば、.tsv の代わりにそれを使います。",
            footer: rsx! {
                button {
                    onclick: {
                        let folder = folder.clone();
                        move |_| {
                            let folder = folder.clone();
                            spawn(async move { pick_and_import_skk(ctx, &folder).await });
                        }
                    },
                    "SKK の辞書を取り込む…"
                }
                button { onclick: move |_| open_folder(&folder), Icon { paths: icons::FOLDER_OPEN } "dictionaries フォルダを開く" }
            },
            OrderedList {
                items,
                chosen,
                fixed: vec![USER_CUSTOM.to_owned()],
                path: path(&["dictionaries"]),
                on_info: move |name: String| looking.set(Some(name)),
                on_convert,
            }
        }
        if let Some((label, path, lookup)) = looked {
            DictionaryEntries {
                key: "{path.display()}",
                label,
                path,
                lookup,
                on_close: move |_| looking.set(None),
            }
        }
        OfficialDictionaries { chosen: chosen_for_official }
        HiddenWords { custom: custom_path }
        PickRecord { path: store_dir.join(SELECTIONS_FILE) }
    }
}

/// The official dictionaries of the latest release, looked up only when the
/// user asks, each with a way to install it or bring it up to date.
#[component]
fn OfficialDictionaries(chosen: Vec<String>) -> Element {
    let ctx = use_context::<Ctx>();
    let dir = ctx.store.read().dir.clone();
    let mut catalog = use_signal(|| None::<Catalog>);
    // What is being fetched: the catalog, or a dictionary by its name.
    let mut busy = use_signal(|| None::<Fetching>);
    let mut error = use_signal(|| None::<String>);
    let check = move |_| {
        spawn(async move {
            busy.set(Some(Fetching::Catalog));
            match in_background(official::fetch_catalog).await {
                Ok(fetched) => {
                    catalog.set(Some(fetched));
                    error.set(None);
                }
                Err(e) => error.set(Some(format!("目録を取れません：{e}"))),
            }
            busy.set(None);
        });
    };
    let install = move |entry: Entry| {
        let dir = dir.clone();
        let chosen = chosen.clone();
        spawn(async move {
            busy.set(Some(Fetching::Dictionary(entry.name.clone())));
            let result = in_background({
                let entry = entry.clone();
                move || entry.install(&dir, &official::fetch(&entry.archive_url())?)
            })
            .await;
            match result {
                Ok(()) => {
                    error.set(None);
                    // The list may have changed during the download; only an
                    // unwritten one falls back to what was read before it.
                    let list =
                        official::placed(written_dictionaries(ctx).unwrap_or(chosen), &entry);
                    ctx.change(&["dictionaries"], Some(list.into_iter().collect()));
                }
                Err(e) => error.set(Some(format!("{} を入れられません：{e}", entry.label))),
            }
            busy.set(None);
        });
    };
    let dir = ctx.store.read().dir.clone();
    rsx! {
        Group {
            title: "公式の辞書",
            note: "かなえみの公式の辞書（kanaemi-dict）を落として入れます。基本辞書は、組になった並べ替えのモデル（ranking.model）と一緒に入れます。追加辞書は、基本辞書に足して使う分野ごとの辞書です。通信するのは、ボタンを押したときだけです。",
            footer: rsx! {
                button { disabled: busy().is_some(), onclick: check,
                    if busy() == Some(Fetching::Catalog) {
                        "確かめています…"
                    } else {
                        "最新の版を確かめる"
                    }
                }
            },
            match catalog() {
                None => rsx! {
                    div { class: "row",
                        div { class: "row-main",
                            span { class: "description", "「最新の版を確かめる」を押すと、入れられる辞書が出ます。" }
                        }
                    }
                },
                Some(catalog) => rsx! {
                    for entry in catalog.dictionaries {
                        OfficialRow {
                            key: "{entry.name}",
                            status: entry.status(&dir),
                            installing: busy() == Some(Fetching::Dictionary(entry.name.clone())),
                            idle: busy().is_none(),
                            entry,
                            on_install: install.clone(),
                        }
                    }
                },
            }
            if let Some(error) = error() {
                p { class: "error", "{error}" }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
enum Fetching {
    Catalog,
    Dictionary(String),
}

#[component]
fn OfficialRow(
    entry: Entry,
    status: Status,
    installing: bool,
    idle: bool,
    on_install: EventHandler<Entry>,
) -> Element {
    let refusal = entry.refusal();
    let (state, action) = match status {
        Status::Missing => ("入れていません", Some("入れる")),
        Status::Outdated => ("新しい版があります", Some("新しくする")),
        Status::Current => ("最新です", None),
    };
    let kind = if entry.base {
        "基本辞書"
    } else {
        "追加辞書"
    };
    let description = refusal
        .clone()
        .unwrap_or_else(|| format!("{kind}・{state}"));
    rsx! {
        div { class: "row",
            div { class: "row-main",
                div { class: "row-text",
                    span { class: "label", "{entry.label}" }
                    span { class: "description", "{description}" }
                }
                span { class: "meta", {size_text(entry.dictionary.size)} }
                div { class: "control",
                    if let Some(action) = action {
                        button {
                            disabled: !idle || refusal.is_some(),
                            onclick: move |_| on_install.call(entry.clone()),
                            if installing {
                                "入れています…"
                            } else {
                                "{action}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The dictionary list the settings file holds now, by name, if it has one.
fn written_dictionaries(ctx: Ctx) -> Option<Vec<String>> {
    let store = ctx.store.read();
    let folder = store.dir.join(DICTIONARY_DIR);
    let sources = store.state.as_ref().ok()?.settings.dictionaries.as_ref()?;
    Some(sources.iter().map(|s| source_name(&folder, s)).collect())
}

/// Runs `work` off the window's thread, so the window keeps answering while
/// it waits on the network.
async fn in_background<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver
        .await
        .unwrap_or_else(|_| Err("途中で止まりました".to_owned()))
}

/// The candidates hidden with the forget key, each with a way back.
#[component]
fn HiddenWords(custom: PathBuf) -> Element {
    // The store is read again when the window comes back, as the IME may
    // have hidden more meanwhile.
    let _ = use_context::<Ctx>().store.read();
    // Bumped after a change to the file, so the list is read again.
    let mut generation = use_signal(|| 0u32);
    let mut error = use_signal(|| None::<String>);
    let _ = generation();
    // Each pair with whether the user registered it: such a word can also go
    // with its registration, as if it had never been registered.
    let pairs: Vec<(String, String, bool)> = TextDictionary::read_user_custom(&custom)
        .map(|(dictionary, _)| {
            let hidden: Vec<(&str, &str)> = dictionary.hidden().collect();
            let mine = registered(&custom, &hidden).unwrap_or_default();
            hidden
                .iter()
                .enumerate()
                .map(|(i, (reading, surface))| {
                    let mine = mine.get(i).copied().unwrap_or(false);
                    ((*reading).to_owned(), (*surface).to_owned(), mine)
                })
                .collect()
        })
        .unwrap_or_default();
    rsx! {
        Group {
            title: "出さない候補",
            note: "変換中に「出さない」（Shift+Delete など）で消した候補です。「戻す」と、また候補に出ます。自分で登録した語は「登録ごと消す」で、登録する前に戻せます。",
            if pairs.is_empty() {
                div { class: "row",
                    div { class: "row-main",
                        span { class: "description", "ありません" }
                    }
                }
            }
            for (reading , surface , mine) in pairs {
                div { class: "row", key: "{reading}\t{surface}",
                    div { class: "row-main",
                        div { class: "row-text",
                            span { class: "label", {show_placeholders(&surface)} }
                            span { class: "description", {show_placeholders(&reading)} }
                        }
                        div { class: "control",
                            if mine {
                                button {
                                    onclick: {
                                        let custom = custom.clone();
                                        let reading = reading.clone();
                                        let surface = surface.clone();
                                        move |_| {
                                            match unregister(&custom, &reading, &surface) {
                                                Ok(()) => error.set(None),
                                                Err(e) => error.set(Some(format!("消せません：{e}"))),
                                            }
                                            generation += 1;
                                        }
                                    },
                                    "登録ごと消す"
                                }
                            }
                            button {
                                onclick: {
                                    let custom = custom.clone();
                                    move |_| {
                                        match unhide(&custom, &reading, &surface) {
                                            Ok(()) => error.set(None),
                                            Err(e) => error.set(Some(format!("戻せません：{e}"))),
                                        }
                                        generation += 1;
                                    }
                                },
                                "戻す"
                            }
                        }
                    }
                }
            }
            if let Some(error) = error() {
                p { class: "error", "{error}" }
            }
        }
    }
}

/// The record of the candidates picked again and again, and a way to erase
/// it. The IME sees the file gone and starts the record over.
#[component]
fn PickRecord(path: PathBuf) -> Element {
    let _ = use_context::<Ctx>().store.read();
    let mut generation = use_signal(|| 0u32);
    let mut error = use_signal(|| None::<String>);
    let _ = generation();
    let kept = path.exists();
    rsx! {
        Group {
            title: "よく選ぶ候補",
            note: "何度も選んだ候補は、その読みで次から先に出ます。この記録は、読みと選んだ候補を、このコンピューターの中にあなたのアカウントだけが読めるファイルとして残します。消すと、最初から覚え直します。",
            div { class: "row",
                div { class: "row-main",
                    div { class: "row-text",
                        span { class: "description",
                            if kept {
                                "記録があります"
                            } else {
                                "記録はありません"
                            }
                        }
                    }
                    div { class: "control",
                        button {
                            disabled: !kept,
                            onclick: move |_| {
                                match fs::remove_file(&path) {
                                    Ok(()) => error.set(None),
                                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => error.set(None),
                                    Err(e) => error.set(Some(format!("消せません：{e}"))),
                                }
                                generation += 1;
                            },
                            "記録を消す"
                        }
                    }
                }
            }
            if let Some(error) = error() {
                p { class: "error", "{error}" }
            }
        }
    }
}

/// Asks for an SKK dictionary and imports it into the dictionaries folder,
/// where it shows like any other dictionary.
async fn pick_and_import_skk(mut ctx: Ctx, folder: &Path) {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("取り込む SKK の辞書")
        .pick_file()
        .await
    else {
        return;
    };
    match import_skk(file.path(), folder) {
        Ok(_) => {
            ctx.errors.write().remove("dictionaries");
        }
        Err(error) => {
            ctx.errors.write().insert(
                "dictionaries".to_owned(),
                format!("取り込めません（{error}）"),
            );
        }
    }
    // The folder now holds a new file.
    ctx.store.write();
}

/// Makes the text dictionary `name` binary, and uses the binary one where the
/// written list used the text one. Without a written list, the binary one
/// stands for the text one by itself.
fn convert_and_use(mut ctx: Ctx, folder: &Path, listed: Option<&[String]>, name: &str) {
    let converted = match convert(folder, name) {
        Ok(converted) => converted,
        Err(error) => {
            ctx.errors.write().insert(
                "dictionaries".to_owned(),
                format!("{name} を変換できません（{error}）"),
            );
            return;
        }
    };
    ctx.errors.write().remove("dictionaries");
    if let Some(chosen) = listed.filter(|chosen| chosen.iter().any(|n| n == name)) {
        let names: Vec<String> = chosen
            .iter()
            .filter(|n| **n != converted)
            .map(|n| {
                if n == name {
                    converted.clone()
                } else {
                    n.clone()
                }
            })
            .collect();
        ctx.change(&["dictionaries"], Some(names.into_iter().collect()));
    } else {
        // The list stays, but the folder now holds a new file.
        ctx.store.write();
    }
}

/// A dictionary file opened to look words up in: the user custom dictionary
/// as the IME reads it, any other as text or binary by its first bytes.
/// Where the dictionary looked into comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Lookup {
    UserCustom,
    File,
    /// A built-in dictionary, by its text.
    Builtin(&'static str),
}

fn open_for_lookup(path: &Path, lookup: Lookup) -> Result<Box<dyn Dictionary>, String> {
    match lookup {
        // Missing until a word is first registered: then it has no words.
        Lookup::UserCustom => TextDictionary::read_user_custom(path)
            .map(|(dictionary, _)| Box::new(dictionary) as Box<dyn Dictionary>)
            .map_err(|e| e.to_string()),
        Lookup::File => open_dictionary(path)
            .map(|(dictionary, _)| dictionary)
            .map_err(|e| e.to_string()),
        Lookup::Builtin(text) => Ok(Box::new(TextDictionary::parse(text).0)),
    }
}

/// How many readings the lookup lists at most, so a large dictionary opens
/// at once.
const READINGS_SHOWN: usize = 100;

/// One word a dictionary has: its reading, its surface, and what else there
/// is to say about it.
type Found = (String, String, Option<String>);

/// The words whose readings start with `query`, from the first reading when
/// it is empty. A query with `*` looks up a word with okurigana by its stem
/// and the okurigana (`か*く`), as a text dictionary writes it.
fn entries_for(dictionary: &dyn Dictionary, query: &str) -> Vec<Found> {
    // As the dictionary holds its readings: a pasted か with its voicing mark
    // apart is が.
    let query: String = query.trim().nfc().collect();
    let query = query.as_str();
    // A `*` may also be part of a reading itself (`あ\*`), so those follow.
    let mut found: Vec<Found> = query
        .split_once('*')
        .and_then(|(stem, okurigana)| Some((stem, okurigana, okurigana.chars().next()?)))
        .map(|(stem, okurigana, kana)| {
            okuri_lookup(dictionary, stem, kana)
                .into_iter()
                .map(|entry| {
                    let surface = format!("{}{okurigana}", show_placeholders(&entry.surface));
                    (query.to_owned(), surface, Some("送り仮名あり".to_owned()))
                })
                .collect()
        })
        .unwrap_or_default();
    // A number's place in a reading is written `{}` but held as a mark.
    let prefix = mark_placeholders(query).unwrap_or_else(|| query.to_owned());
    found.extend(
        dictionary
            .readings_from(&prefix, READINGS_SHOWN)
            .into_iter()
            .flat_map(|reading| {
                let shown = show_placeholders(&reading);
                dictionary.lookup(&reading).into_iter().map(move |entry| {
                    let note = entry.conjugation.map(|c| format!("活用：{c}"));
                    (shown.clone(), show_placeholders(&entry.surface), note)
                })
            }),
    );
    found
}

/// A dictionary file as opened, with the time it was last changed then.
type Opened = (Option<SystemTime>, Rc<Result<Box<dyn Dictionary>, String>>);

/// What a dictionary has for a reading, to see before choosing it.
#[component]
fn DictionaryEntries(
    label: String,
    path: PathBuf,
    lookup: Lookup,
    on_close: EventHandler<()>,
) -> Element {
    // The store is read again when the window comes back, as a word may have
    // been registered meanwhile; the file is opened again only if it changed.
    let _ = use_context::<Ctx>().store.read();
    let opened = use_hook(|| Rc::new(RefCell::new(None::<Opened>)));
    let stamp = fs::metadata(&path).and_then(|m| m.modified()).ok();
    let dictionary = {
        let mut opened = opened.borrow_mut();
        match &*opened {
            Some((at, dictionary)) if *at == stamp => dictionary.clone(),
            _ => {
                let dictionary = Rc::new(open_for_lookup(&path, lookup));
                *opened = Some((stamp, dictionary.clone()));
                dictionary
            }
        }
    };
    let mut reading = use_signal(String::new);
    let found = match dictionary.as_ref() {
        Ok(dictionary) => Ok(entries_for(dictionary.as_ref(), &reading())),
        Err(error) => Err(error.clone()),
    };
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "modal",
                onclick: move |e| e.stop_propagation(),
                header {
                    div {
                        h2 { "{label}" }
                        p { class: "item-description",
                            "読みの順に、はじめの {READINGS_SHOWN} 個の読みの語を出します。読みをかなで入れると、その読みで始まる語に絞ります。送り仮名のある語は、送り仮名の前に * を入れて引きます（か*く）。"
                        }
                    }
                    button { onclick: move |_| on_close.call(()), "閉じる" }
                }
                input {
                    class: "filter",
                    placeholder: "読みで絞り込む…",
                    value: "{reading}",
                    oninput: move |e| reading.set(e.value()),
                }
                div { class: "modal-body",
                    match found {
                        Err(error) => rsx! { p { class: "error", "読めません：{error}" } },
                        Ok(found) if found.is_empty() => rsx! {
                            p { class: "description", "語はありません" }
                        },
                        Ok(found) => rsx! {
                            table { class: "rules",
                                thead {
                                    tr {
                                        th { "読み" }
                                        th { "表記" }
                                        th {}
                                    }
                                }
                                tbody {
                                    for (reading , surface , note) in found {
                                        tr {
                                            td { "{reading}" }
                                            td { "{surface}" }
                                            td { class: "item-description", {note.unwrap_or_default()} }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str, query: &str) -> Vec<(String, String)> {
        entries_for(&TextDictionary::parse(text).0, query)
            .into_iter()
            .map(|(reading, surface, _)| (reading, surface))
            .collect()
    }

    fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn the_first_readings_are_shown_with_nothing_asked() {
        assert_eq!(
            found("かんじ\t漢字\nあい\t愛", ""),
            pairs(&[("あい", "愛"), ("かんじ", "漢字")])
        );
    }

    #[test]
    fn a_query_narrows_to_the_readings_it_starts() {
        assert_eq!(
            found("かんじ\t漢字\nかんじ\t感じ\nあい\t愛", " かん "),
            pairs(&[("かんじ", "感じ"), ("かんじ", "漢字")]),
            "cheapest first, as the dictionary has them"
        );
    }

    #[test]
    fn a_reading_with_its_voicing_mark_apart_is_found_as_one() {
        assert_eq!(found("がく\t学", "か\u{3099}"), pairs(&[("がく", "学")]));
        assert_eq!(
            found("が*く\t欠く", "か\u{3099}*く"),
            pairs(&[("が*く", "欠く")]),
            "the row of が, not of か"
        );
    }

    #[test]
    fn a_reading_with_a_star_of_its_own_is_found_too() {
        assert_eq!(found("あ\\*\t亜", "あ*"), pairs(&[("あ*", "亜")]));
    }

    #[test]
    fn a_reading_with_a_number_is_shown_and_found_as_written() {
        assert_eq!(found("{}こ\t{}個", "{}"), pairs(&[("{}こ", "{}個")]));
    }

    #[test]
    fn a_word_with_okurigana_is_found_by_its_stem_and_okurigana() {
        assert_eq!(found("か*く\t書く", "か*く"), pairs(&[("か*く", "書く")]));
        assert_eq!(found("か*く\t書く", "か*"), []);
    }
}
