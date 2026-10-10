//! What the settings app says about a setting it cannot use, in plain
//! words: the file's own terms are for the people who edit it by hand.

use kanaemi_config::ProblemKind;
use kanaemi_engine::WordError;

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
        ProblemKind::UnknownBuiltinDictionary(name) => {
            format!("「{name}」という組み込みの辞書はありません")
        }
        ProblemKind::NotSendable => {
            "アプリに送れるのは、Backspace や矢印のような名前のあるキーだけです".to_owned()
        }
        ProblemKind::OutsideFolder(name) => {
            format!("「{name}」はフォルダの外を指しています（フォルダの中の名前で書きます）")
        }
    }
}

/// Why a word of the user dictionary was not written.
pub fn describe_word(error: &WordError) -> String {
    match error {
        WordError::EmptyReading => "読みを入れてください".to_owned(),
        WordError::NotKana(c) if c.is_whitespace() => "読みに空白は入れられません".to_owned(),
        WordError::NotKana(c) => {
            format!("読みはひらがなで入れてください（「{c}」は使えません）")
        }
        WordError::Brace => "読みの数は {} と書いてください".to_owned(),
        WordError::ManyMarks => "送り仮名の前に入れる * は 1 つだけにしてください".to_owned(),
        WordError::EmptyStem => "* の前に、送り仮名の前までの読みを入れてください".to_owned(),
        WordError::Okurigana => {
            "* の後ろには、送り仮名の最初のかなを 1 文字だけ入れてください（か*く）".to_owned()
        }
        WordError::OkuriganaWithPlaceholder => {
            "送り仮名のある語には、数の {} も置き場所も使えません".to_owned()
        }
        WordError::EmptySurface => "表記を入れてください".to_owned(),
        WordError::Unusable(c) if c.is_control() => "表記にタブや改行は入れられません".to_owned(),
        WordError::Unusable(c) => format!("表記に {} は使えません", c.escape_unicode()),
        WordError::SurfaceOkurigana(kana) => {
            format!(
                "送り仮名のある語は、表記も「{kana}」で終わるように書いてください（か*く なら 書く）"
            )
        }
        WordError::Placeholders => {
            "置き場所が、読みにない数を使っています。読みに数の {} を足してください".to_owned()
        }
        WordError::UnknownFunction(name) => format!(
            "「{name}」という関数はありません。組み込みの関数か、functions フォルダの関数の名前を書いてください"
        ),
        WordError::Invalid => "辞書の行として読めない語です".to_owned(),
        WordError::Exists => "同じ読みと表記の語が、もう登録してあります".to_owned(),
        WordError::Gone => "直す前の語が見つかりません。ほかで書き換えられたようです".to_owned(),
        WordError::Io(e) => format!("書けません：{e}"),
    }
}
