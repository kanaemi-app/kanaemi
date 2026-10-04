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
        }
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
            });
        }
    }
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
                on_convert,
            }
        }
        HiddenWords { custom: custom_path }
        PickRecord { path: store_dir.join(SELECTIONS_FILE) }
    }
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
    let pairs: Vec<(String, String)> = TextDictionary::read_user_custom(&custom)
        .map(|(dictionary, _)| {
            dictionary
                .hidden()
                .map(|(reading, surface)| (reading.to_owned(), surface.to_owned()))
                .collect()
        })
        .unwrap_or_default();
    rsx! {
        Group {
            title: "出さない候補",
            note: "変換中に「出さない」（Shift+Delete など）で消した候補です。「戻す」と、また候補に出ます。",
            if pairs.is_empty() {
                div { class: "row",
                    div { class: "row-main",
                        span { class: "description", "ありません" }
                    }
                }
            }
            for (reading , surface) in pairs {
                div { class: "row", key: "{reading}\t{surface}",
                    div { class: "row-main",
                        div { class: "row-text",
                            span { class: "label", {show_placeholders(&surface)} }
                            span { class: "description", {show_placeholders(&reading)} }
                        }
                        div { class: "control",
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
