use super::words::UserWords;
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
    items.extend(file_items(&folder, &files, &chosen));
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
            None => (
                items
                    .iter()
                    .find(|i| i.name == name)
                    .map_or_else(|| name.clone(), |i| i.label.clone()),
                folder.join(&name),
                Lookup::File,
            ),
        }
    });
    let on_convert = {
        let folder = folder.clone();
        let listed = written.is_some().then(|| chosen.clone());
        move |name: String| {
            let source = conversion_source(&folder, &name);
            convert_and_use(ctx, &folder, listed.as_deref(), &source)
        }
    };
    rsx! {
        Group {
            title: "使う辞書",
            note: "上の辞書ほど候補が先に出ます。つまみをドラッグして順番を変え、スイッチで使うかどうかを決めます。dictionaries フォルダに置いた辞書（.tsv と .kdic）も、ここに出ます。",
            footer: rsx! {
                button {
                    onclick: {
                        let folder = folder.clone();
                        move |_| open_folder(&folder)
                    },
                    Icon { paths: icons::FOLDER_OPEN } "dictionaries フォルダを開く" }
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
        match looked {
            Some((_, _, Lookup::UserCustom)) => rsx! {
                UserWords {
                    custom: custom_path.clone(),
                    functions: store_dir.join(FUNCTIONS_DIR),
                    on_close: move |_| looking.set(None),
                }
            },
            Some((label, path, lookup)) => rsx! {
                DictionaryEntries {
                    key: "{path.display()}",
                    label,
                    path,
                    lookup,
                    on_close: move |_| looking.set(None),
                }
            },
            None => rsx! {},
        }
        OtherDictionaries { folder: folder.clone(), custom: custom_path.clone() }
        OfficialDictionaries { chosen: chosen_for_official }
        HiddenWords { custom: custom_path }
        PickRecord { path: store_dir.join(SELECTIONS_FILE) }
    }
}

/// A row for each dictionary file in `folder`, named by what its text says it
/// is. A binary dictionary stands for the text one it was made from, which is
/// listed apart only while `chosen` names it.
fn file_items(folder: &Path, files: &[String], chosen: &[String]) -> Vec<ListItem> {
    let text_of = |name: &str| {
        let text = Path::new(name)
            .with_extension(TEXT_EXTENSION)
            .to_string_lossy()
            .into_owned();
        (text != name && files.contains(&text) && is_binary(&folder.join(name))).then_some(text)
    };
    files
        .iter()
        .filter(|name| {
            chosen.contains(name) || text_of(&binary_name(name.as_str())).as_ref() != Some(name)
        })
        .map(|name| {
            let path = folder.join(name);
            let binary = is_binary(&path);
            let text = text_of(name);
            let status = match &text {
                Some(text) => Some(conversion(folder, text)),
                None if !binary && binary_name(name) != *name => Some(conversion(folder, name)),
                None => None,
            };
            let (meta, convert) = match status {
                Some(Conversion::None) => (file_size(&path), Some("バイナリに変換")),
                Some(Conversion::Stale) => (
                    file_size(&path).map(|size| format!("要再変換・{size}")),
                    Some("バイナリに再変換"),
                ),
                Some(Conversion::Current) if !binary => (
                    file_size(&path).map(|size| format!("変換済み・{size}")),
                    None,
                ),
                Some(Conversion::Current) | None => (file_size(&path), None),
            };
            let said = text_description(&folder.join(text.as_deref().unwrap_or(name)));
            let (label, description) = match said {
                Some(said) => (said, Some(name.clone())),
                None if binary => (name.clone(), Some("バイナリの辞書".to_owned())),
                None => (name.clone(), None),
            };
            ListItem {
                name: name.clone(),
                label,
                description,
                meta,
                convert: convert.map(str::to_owned),
                warning: (!binary).then(|| invalid_lines(&path, false)).flatten(),
                chip: None,
            }
        })
        .collect()
}

/// The text dictionary a conversion of the dictionary `name` reads: the one
/// a binary dictionary was made from, or `name` itself.
fn conversion_source(folder: &Path, name: &str) -> String {
    let text = Path::new(name)
        .with_extension(TEXT_EXTENSION)
        .to_string_lossy()
        .into_owned();
    if text != name && is_binary(&folder.join(name)) && folder.join(&text).is_file() {
        text
    } else {
        name.to_owned()
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

/// The dictionaries of other input methods, each taken in as a dictionary of
/// its own after a look at what it takes and leaves out, and the user custom
/// dictionary written out as them.
#[component]
fn OtherDictionaries(folder: PathBuf, custom: PathBuf) -> Element {
    let mut ctx = use_context::<Ctx>();
    let mut format = use_signal(|| DictionaryFormat::offered()[0]);
    // How the last import or export went, or why it failed.
    let mut outcome = use_signal(|| None::<Result<String, String>>);
    // An import read and waiting to be confirmed.
    let mut previewing = use_signal(|| None::<ImportPreview>);
    let import = move |_| {
        spawn(async move {
            let format = format();
            let source = match format.finding() {
                Finding::Known(source) => source,
                Finding::Missing(message) => {
                    outcome.set(Some(Err(message)));
                    return;
                }
                Finding::Pick(folder) => {
                    let dialog = rfd::AsyncFileDialog::new()
                        .set_title(format!("取り込む {} の辞書", format.label()));
                    let dialog = match folder {
                        Some(folder) => dialog.set_directory(folder),
                        None => dialog,
                    };
                    let Some(file) = dialog.pick_file().await else {
                        return;
                    };
                    Source::picked(file.path().to_owned())
                }
            };
            match in_background(move || preview_import(&source, format)).await {
                Ok(preview) => {
                    outcome.set(None);
                    previewing.set(Some(preview));
                }
                Err(e) => outcome.set(Some(Err(format!("取り込めません（{e}）")))),
            }
        });
    };
    let confirm = move |_| {
        let folder = folder.clone();
        let Some(preview) = previewing.take() else {
            return;
        };
        spawn(async move {
            outcome.set(Some(
                in_background(move || write_import(&preview, &folder))
                    .await
                    .map_err(|e| format!("取り込めません（{e}）")),
            ));
            // The folder now holds a new file.
            ctx.store.write();
        });
    };
    let export = move |_| {
        let custom = custom.clone();
        spawn(async move {
            let format = format();
            let dialog = rfd::AsyncFileDialog::new()
                .set_title(format!("{} の形式で書き出す", format.label()))
                .set_file_name(format.file_name());
            let dialog = match format.export_folder().filter(|f| f.is_dir()) {
                Some(folder) => dialog.set_directory(folder),
                None => dialog,
            };
            let Some(file) = dialog.save_file().await else {
                return;
            };
            let target = file.path().to_owned();
            outcome.set(Some(
                in_background(move || export_dictionary(&custom, &target, format))
                    .await
                    .map_err(|e| format!("書き出せません（{e}）")),
            ));
        });
    };
    let finding = format().finding();
    let picks = matches!(finding, Finding::Pick(_));
    let missing = match finding {
        Finding::Missing(message) => Some(message),
        _ => None,
    };
    rsx! {
        Group {
            title: "ほかの IME の辞書",
            note: "ほかの IME の辞書を、別の辞書として取り込みます。ユーザー辞書の語を、その形式で書き出すこともできます。",
            div { class: "row",
                div { class: "row-main",
                    div { class: "row-text",
                        span { class: "label", "形式" }
                        span { class: "description", {format_note(format())} }
                        if let Some(missing) = missing.as_ref() {
                            span { class: "description", "{missing}。取り込めませんが、書き出せます。" }
                        }
                    }
                    div { class: "control",
                        Select {
                            choices: DictionaryFormat::offered()
                                .into_iter()
                                .map(|f| Choice {
                                    value: f.label().to_owned(),
                                    label: f.label().to_owned(),
                                    description: Some(f.description().to_owned()),
                                })
                                .collect::<Vec<_>>(),
                            value: format().label(),
                            onchange: move |value: String| {
                                if let Some(f) = DictionaryFormat::all().into_iter().find(|f| f.label() == value) {
                                    format.set(f);
                                }
                            },
                        }
                        button {
                            // An SKK implementation not on this machine has nothing to
                            // take in, though a dictionary can be written out for it.
                            disabled: missing.is_some(),
                            onclick: import,
                            if picks { "取り込む…" } else { "取り込む" }
                        }
                        button { onclick: export, "書き出す…" }
                    }
                }
            }
            match outcome() {
                Some(Ok(message)) => rsx! {
                    p { class: "note", "{message}" }
                },
                Some(Err(error)) => rsx! {
                    p { class: "error", "{error}" }
                },
                None => rsx! {},
            }
        }
        if let Some(preview) = previewing() {
            ImportPreviewModal {
                preview,
                on_import: confirm,
                on_close: move |_| previewing.set(None),
            }
        }
    }
}

/// Where the import reads, and what to do with the file written out.
fn format_note(format: DictionaryFormat) -> String {
    let skk_export = match format {
        DictionaryFormat::Skk(skk) if skk.encoding() == SkkEncoding::EucJp => {
            "書き出すファイルは EUC-JP の SKK 辞書です。"
        }
        _ => "書き出すファイルは UTF-8 の SKK 辞書です。",
    };
    match format {
        DictionaryFormat::Ime(ImeFormat::MsIme) => {
            "ユーザー辞書ツールの「一覧の出力」で書き出したテキストを選びます。書き出したファイルは「テキストファイルからの登録」で読み込めます。".to_owned()
        }
        DictionaryFormat::Ime(ImeFormat::Google) => {
            "辞書ツールの「エクスポート」で書き出したテキストを選びます。書き出したファイルは「インポート」で読み込めます。".to_owned()
        }
        DictionaryFormat::Ime(ImeFormat::Atok) => {
            "ATOK の辞書ユーティリティで単語の一覧を書き出したテキストを選びます。書き出したファイルも、辞書ユーティリティで登録できます。".to_owned()
        }
        DictionaryFormat::Ime(ImeFormat::MacOs) if cfg!(target_os = "macos") => {
            "システム設定の「キーボード」のユーザ辞書（テキスト置換）を、そのまま読みます。書き出した plist は、その一覧にドラッグすると読み込めます。".to_owned()
        }
        DictionaryFormat::Ime(ImeFormat::MacOs) => {
            "macOS のユーザ辞書（テキスト置換）の一覧から、項目をドラッグして書き出した plist を選びます。書き出したファイルは、その一覧にドラッグすると読み込めます。".to_owned()
        }
        DictionaryFormat::Skk(SkkSource::File) => format!(
            "SKK-JISYO.L のような辞書や、SKK のユーザー辞書を選びます。文字コードは 1 行目の coding: で決まり、ないときは EUC-JP です。{skk_export}"
        ),
        DictionaryFormat::Skk(SkkSource::MacSkk) => format!(
            "macSKK のユーザー辞書（skk-jisyo.utf8）を、そのまま読みます。{skk_export}macSKK の辞書のフォルダに書き出すと、macSKK の設定の「辞書」で使えます。"
        ),
        DictionaryFormat::Skk(_) => format!(            "{} のユーザー辞書を、そのまま読みます。{skk_export}",
            format.label()
        ),
    }
}

/// How many rows of each list the preview of an import draws; a large SKK
/// dictionary has far more than anyone reads through.
const PREVIEW_ROWS: usize = 300;

/// What an import takes and leaves out, shown before it writes anything.
#[component]
fn ImportPreviewModal(
    preview: ImportPreview,
    on_import: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let mut showing_skipped = use_signal(|| false);
    let words = preview.word_count();
    let skipped = preview.skipped.len();
    let summary = if skipped == 0 {
        format!("{words} 語を取り込みます。")
    } else {
        format!("{words} 語を取り込みます。{skipped} 件は取り込みません。")
    };
    let more = |total: usize| total.saturating_sub(PREVIEW_ROWS);
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "modal",
                onclick: move |e| e.stop_propagation(),
                header {
                    div {
                        h2 { "{preview.name} を取り込む" }
                        p { class: "item-description",
                            "{summary}取り込むと、dictionaries フォルダに別の辞書として置きます。"
                        }
                    }
                }
                div { class: "view-switch",
                    button {
                        class: if !showing_skipped() { "selected" } else { "" },
                        onclick: move |_| showing_skipped.set(false),
                        "取り込む語（{words}）"
                    }
                    button {
                        class: if showing_skipped() { "selected" } else { "" },
                        onclick: move |_| showing_skipped.set(true),
                        "取り込まないもの（{skipped}）"
                    }
                }
                div { class: "modal-body",
                    if !showing_skipped() {
                        if words == 0 {
                            p { class: "description", "取り込める語はありません" }
                        } else {
                            table { class: "rules",
                                thead {
                                    tr {
                                        th { "読み" }
                                        th { "表記" }
                                        th { "活用" }
                                    }
                                }
                                tbody {
                                    for (reading , surface , conjugation) in preview.words().take(PREVIEW_ROWS) {
                                        tr {
                                            td { "{reading}" }
                                            td { "{surface}" }
                                            td { class: "item-description", "{conjugation}" }
                                        }
                                    }
                                }
                            }
                            if more(words) > 0 {
                                p { class: "description", "ほか {more(words)} 語" }
                            }
                        }
                    } else if skipped == 0 {
                        p { class: "description", "取り込まないものはありません" }
                    } else {
                        table { class: "rules",
                            thead {
                                tr {
                                    th { "行" }
                                    th { "内容" }
                                    th { "わけ" }
                                }
                            }
                            tbody {
                                for skip in preview.skipped.iter().take(PREVIEW_ROWS) {
                                    tr {
                                        td { "{skip.line}" }
                                        td { "{skip.text}" }
                                        td { class: "item-description", {skip_reason(skip.reason)} }
                                    }
                                }
                            }
                        }
                        if more(skipped) > 0 {
                            p { class: "description", "ほか {more(skipped)} 件" }
                        }
                    }
                }
                div { class: "modal-footer",
                    button { onclick: move |_| on_close.call(()), "キャンセル" }
                    button {
                        class: "primary",
                        disabled: words == 0,
                        onclick: move |_| on_import.call(()),
                        "取り込む"
                    }
                }
            }
        }
    }
}

fn skip_reason(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Unreadable => "読みか語がない",
        SkipReason::Unrepresentable => "かなえみで表せない",
        SkipReason::Hidden => "抑制単語は、取り込んだ辞書では隠せない",
    }
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
                Filter { placeholder: "読みで絞り込む…", oninput: move |text| reading.set(text) }
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

    fn folder_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "kanaemi-settings-dictionaries-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        for (file, text) in files {
            fs::write(folder.join(file), text).unwrap();
        }
        folder
    }

    fn items_in(folder: &Path, chosen: &[&str]) -> Vec<ListItem> {
        let chosen: Vec<String> = chosen.iter().map(|n| (*n).to_owned()).collect();
        file_items(folder, &dictionary_files(folder), &chosen)
    }

    #[test]
    fn a_text_dictionary_is_named_by_its_first_line_with_its_file_beside() {
        let folder = folder_with("text", &[("a.tsv", "# 地名の辞書\nあ\t亜\n")]);

        let items = items_in(&folder, &[]);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "a.tsv");
        assert_eq!(items[0].label, "地名の辞書");
        assert_eq!(items[0].description.as_deref(), Some("a.tsv"));
        assert_eq!(items[0].convert.as_deref(), Some("バイナリに変換"));
    }

    #[test]
    fn a_dictionary_saying_nothing_of_itself_is_named_by_its_file_alone() {
        let folder = folder_with("bare", &[("a.tsv", "あ\t亜\n")]);

        let items = items_in(&folder, &[]);

        assert_eq!(items[0].label, "a.tsv");
        assert_eq!(items[0].description, None);
    }

    #[test]
    fn a_binary_dictionary_is_named_by_the_text_it_was_made_from_and_stands_for_it() {
        let folder = folder_with("binary", &[("a.tsv", "# 地名の辞書\nあ\t亜\n")]);
        convert(&folder, "a.tsv").unwrap();

        let items = items_in(&folder, &[]);

        assert_eq!(items.len(), 1, "the text one is not listed apart");
        assert_eq!(items[0].name, "a.kdic");
        assert_eq!(items[0].label, "地名の辞書");
        assert_eq!(items[0].description.as_deref(), Some("a.kdic"));
        assert_eq!(items[0].convert, None);
    }

    #[test]
    fn a_text_dictionary_chosen_by_name_stays_beside_its_binary_one() {
        let folder = folder_with("chosen", &[("a.tsv", "# 地名の辞書\nあ\t亜\n")]);
        convert(&folder, "a.tsv").unwrap();

        let names: Vec<String> = items_in(&folder, &["a.tsv"])
            .into_iter()
            .map(|i| i.name)
            .collect();

        assert_eq!(names, ["a.kdic", "a.tsv"]);
    }

    #[test]
    fn a_binary_dictionary_older_than_its_text_offers_to_be_made_again() {
        let folder = folder_with("stale", &[("a.tsv", "# 地名の辞書\nあ\t亜\n")]);
        convert(&folder, "a.tsv").unwrap();
        fs::write(folder.join("a.tsv"), "# 地名の辞書\nい\t伊\n").unwrap();

        let items = items_in(&folder, &[]);

        assert_eq!(items[0].name, "a.kdic");
        assert_eq!(items[0].convert.as_deref(), Some("バイナリに再変換"));
        assert!(items[0].meta.as_deref().unwrap().starts_with("要再変換"));
        assert_eq!(conversion_source(&folder, "a.kdic"), "a.tsv");
    }

    #[test]
    fn a_binary_dictionary_without_its_text_says_it_is_binary() {
        let folder = folder_with("lone", &[("a.tsv", "# 地名の辞書\nあ\t亜\n")]);
        convert(&folder, "a.tsv").unwrap();
        fs::remove_file(folder.join("a.tsv")).unwrap();

        let items = items_in(&folder, &[]);

        assert_eq!(items[0].label, "a.kdic");
        assert_eq!(items[0].description.as_deref(), Some("バイナリの辞書"));
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
