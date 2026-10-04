//! What the settings app says about a setting it cannot use, in plain
//! words: the file's own terms are for the people who edit it by hand.

use kanaemi_config::ProblemKind;

pub fn describe(kind: &ProblemKind) -> String {
    match kind {
        ProblemKind::Syntax(message) => {
            format!("設定ファイルの書き方が TOML として正しくありません（{message}）")
        }
        ProblemKind::UnknownItem => "かなえみの知らない設定です".to_owned(),
        ProblemKind::NotATable => "[ ] で始まる節として書いてください".to_owned(),
        ProblemKind::NotStrings => "[\"a\", \"b\"] のような文字列の一覧で書いてください".to_owned(),
        ProblemKind::NotAString => "\" \" で囲んだ文字列で書いてください".to_owned(),
        ProblemKind::NotABool => "true か false で書いてください".to_owned(),
        ProblemKind::BadMark => "空でない 1 行の文字にしてください".to_owned(),
        ProblemKind::UnknownKey(key) => format!("「{key}」というキーはありません"),
        ProblemKind::UnknownModifier(name) => {
            format!("「{name}」は修飾キーではありません（cmd・ctrl・alt から選べます）")
        }
        ProblemKind::RomajiUnreadable { table, error } => {
            format!("ローマ字の表「{table}」を開けません（{error}）")
        }
        ProblemKind::RomajiInvalidLines { table, lines } => {
            let lines: Vec<String> = lines.iter().map(usize::to_string).collect();
            format!(
                "ローマ字の表「{table}」の {} 行目を読めません",
                lines.join("・")
            )
        }
        ProblemKind::ModifierKey => "修飾キーは、別のキーに置き換えてアプリに送れません".to_owned(),
        ProblemKind::NotAPositiveInteger => "1 以上の整数で書いてください".to_owned(),
        ProblemKind::NotAPort => "1 から 65535 までの整数で書いてください".to_owned(),
        ProblemKind::UnknownAction(name) => {
            format!("「{name}」という機能はありません（@next のように書きます）")
        }
        ProblemKind::ActionNotHere(name) => format!("「{name}」はこの場面では使えません"),
        ProblemKind::NotSendable => {
            "アプリに送れるのは、Backspace や矢印のような名前のあるキーだけです".to_owned()
        }
        ProblemKind::OutsideFolder(name) => {
            format!("「{name}」はフォルダの外を指しています（フォルダの中の名前で書きます）")
        }
    }
}
