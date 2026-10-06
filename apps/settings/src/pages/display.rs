use super::*;

#[component]
pub fn Display() -> Element {
    let ctx = use_context::<Ctx>();
    let config = config(ctx);
    let shipped = Config::default();
    let marks = &config.marks;
    let m = &shipped.marks;
    let rows = [
        (
            "reading",
            "読み",
            "変換する読みを打っているとき、その前に付けます",
            &marks.reading,
            &m.reading,
            format!("{}かんじ", marks.reading),
        ),
        (
            "candidate",
            "候補",
            "変換の候補を選んでいるとき、その前に付けます",
            &marks.candidate,
            &m.candidate,
            format!("{}漢字", marks.candidate),
        ),
        (
            "okurigana",
            "送り仮名",
            "送り仮名の始まりに付けます",
            &marks.okurigana,
            &m.okurigana,
            format!("{}か{}き", marks.reading, marks.okurigana),
        ),
        (
            "registration",
            "登録",
            "辞書に語を登録しているとき、読みと登録する語のあいだに入れます",
            &marks.registration,
            &m.registration,
            format!("{}かなえみ{}", marks.candidate, marks.registration),
        ),
        (
            "cursor",
            "カーソル",
            "読みの途中にカーソルを戻したとき、その位置に入れます",
            &marks.cursor,
            &m.cursor,
            format!("{}か{}んじ", marks.reading, marks.cursor),
        ),
        (
            "hold",
            "押さえたまま",
            "押さえたままに割り当てたキーを、押さえたままか単独で押したのか決まるまで、末尾に付けます。空にすると、見えない幅のない文字を付けます",
            &marks.hold,
            &m.hold,
            format!("{}かん{}j", marks.reading, marks.hold),
        ),
    ];
    rsx! {
        Group {
        Row {
            label: "入力モードを表示",
            description: "かなと ABC を切り替えたとき、カーソルのそばに「かな」「ABC」と少しのあいだ表示します。",
            path: path(&["mode_indicator"]),
            shipped: (config.mode_indicator != shipped.mode_indicator)
                .then(|| on_off(shipped.mode_indicator)),
            input {
                class: "switch",
                r#type: "checkbox",
                role: "switch",
                checked: config.mode_indicator,
                onchange: move |e| ctx.change(&["mode_indicator"], Some(e.checked().into())),
            }
        }
        }
        Group {
            title: "変換中の文字に付ける印",
            note: "変換している途中の文字に印を付けて、いまの状態を見分けやすくします。",
        for (key , label , description , value , default , example) in rows {
            Row {
                key: "{key}",
                label,
                description: description.to_owned(),
                path: path(&["marks", key]),
                shipped: (value != default).then(|| default.clone()),
                span { class: "example", "{example}" }
                input {
                    class: "mark",
                    value: "{value}",
                    oninput: move |e| ctx.change(&["marks", key], Some(e.value().into())),
                }
            }
        }
        }
    }
}

fn on_off(on: bool) -> String {
    if on {
        "表示する"
    } else {
        "表示しない"
    }
    .to_owned()
}
