use super::*;
use crate::Dictionary;

fn utf16le(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    bytes
}

fn lines_of(read: &ImeWords) -> Vec<&str> {
    read.words
        .iter()
        .flat_map(|w| w.lines.iter().map(String::as_str))
        .collect()
}

fn skipped(line: usize, reason: SkipReason) -> (usize, SkipReason) {
    (line, reason)
}

fn reasons(read: &ImeWords) -> Vec<(usize, SkipReason)> {
    read.skipped.iter().map(|s| (s.line, s.reason)).collect()
}

fn custom(text: &str) -> TextDictionary {
    let (dictionary, invalid) = TextDictionary::parse_user_custom(text);
    assert_eq!(invalid, [], "{text}");
    dictionary
}

#[test]
fn an_ms_ime_list_in_utf_16_is_read_with_its_parts_of_speech() {
    let file = utf16le(
        "!Microsoft IME Dictionary Tool\r\n!Format:WORDLIST\r\n\r\n\
         きしゃ\t記者\t名詞\r\n\
         かく\t書く\tか行五段\r\n\
         たべる\t食べる\t一段動詞\r\n\
         みる\t見る\t一段動詞\r\n\
         たかい\t高い\t形容詞\r\n\
         べんきょう\t勉強\tさ変名詞\r\n\
         いく\t行く\tか行五段\r\n\
         とう\t問う\tあわ行う音便\r\n\
         きしゃ\t汽車\t抑制単語\r\n\
         あいする\t愛する\tサ変動詞\r\n\
         にほん\t日本\t地名その他\r\n",
    );
    let read = read_ime_dictionary(file, ImeFormat::MsIme).unwrap();
    assert_eq!(reasons(&read), [skipped(12, SkipReason::Hidden)]);
    assert_eq!(
        lines_of(&read),
        [
            "きしゃ\t記者",
            "か\t書\t五段-カ行",
            "たべ\t食べ\t下一段-バ行",
            "み\t見\t上一段-マ行",
            "たか\t高\t形容詞",
            "べんきょう\t勉強",
            "べんきょう\t勉強\tサ行変格",
            "い\t行\t五段-カ行-促音便",
            "と\t問\t五段-ワア行-ウ音便",
            "あい\t愛\tサ行変格",
            "にほん\t日本",
        ]
    );
    assert_eq!(read.words[0].line, 4);
    assert_eq!(read.words[5].lines.len(), 2, "a サ変名詞 is one word");
}

#[test]
fn a_part_of_speech_that_does_not_fit_the_word_is_dropped() {
    let read = read_ime_dictionary(
        "かく\t書\tか行五段\nくる\t来る\tカ変動詞\nきれい\t綺麗\t形容動詞\nる\tる\t一段動詞\n",
        ImeFormat::Google,
    )
    .unwrap();
    assert_eq!(
        lines_of(&read),
        ["かく\t書", "くる\t来る", "きれい\t綺麗", "る\tる"]
    );
}

#[test]
fn readings_are_made_hiragana_and_untypable_ones_are_left_out() {
    let read = read_ime_dictionary(
        "カタカナ\t片仮名\t名詞\n\
         ｶﾞｯｺｳ\t学校\t名詞\n\
         ヴぁ\tヴァ\t名詞\n\
         1ねん\t一年\t名詞\n\
         abc\tABC\t名詞\n\
         かん字\t漢字\t名詞\n\
         ヷ\tヷ\t名詞\n\
         ひと り\t一人\t名詞\n",
        ImeFormat::Google,
    )
    .unwrap();
    assert_eq!(
        lines_of(&read),
        [
            "かたかな\t片仮名",
            "がっこう\t学校",
            "ゔぁ\tヴァ",
            "１ねん\t一年"
        ]
    );
    assert_eq!(
        reasons(&read),
        [5, 6, 7, 8].map(|line| skipped(line, SkipReason::Unrepresentable))
    );
}

#[test]
fn lines_without_a_reading_or_a_word_are_unreadable() {
    let read = read_ime_dictionary(
        "# comment\nきしゃ\nきしゃ\t\n\t記者\nきしゃ\t記者\n\n",
        ImeFormat::Google,
    )
    .unwrap();
    assert_eq!(lines_of(&read), ["きしゃ\t記者"]);
    assert_eq!(
        reasons(&read),
        [2, 3, 4].map(|line| skipped(line, SkipReason::Unreadable))
    );
}

#[test]
fn a_line_left_out_keeps_what_it_held_to_show() {
    let read = read_ime_dictionary("abc\tABC\t名詞\nきしゃ\n", ImeFormat::Google).unwrap();
    let texts: Vec<&str> = read.skipped.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(texts, ["abc\tABC\t名詞", "きしゃ"]);
}

#[test]
fn text_replacements_are_read_by_shortcut_and_phrase() {
    let read = read_text_replacements([
        ("きしゃ".to_owned(), "記者".to_owned()),
        ("omw".to_owned(), "On my way!".to_owned()),
    ]);
    assert_eq!(lines_of(&read), ["きしゃ\t記者"]);
    assert_eq!(reasons(&read), [skipped(2, SkipReason::Unrepresentable)]);
    assert_eq!(read.skipped[0].text, "omw\tOn my way!");
}

#[test]
fn a_word_with_placeholder_characters_is_left_out_and_braces_stay_letters() {
    let read = read_ime_dictionary(
        "かお\t{^_^}\t顔文字\nだめ\ta\u{fdd0}b\t名詞\n",
        ImeFormat::Google,
    )
    .unwrap();
    assert_eq!(lines_of(&read), ["かお\t\\{^_^}"]);
    assert_eq!(reasons(&read), [skipped(2, SkipReason::Unrepresentable)]);
    let d = custom(&lines_of(&read).join("\n"));
    assert_eq!(d.lookup("かお")[0].surface, "{^_^}");
}

#[test]
fn comment_lines_differ_by_format() {
    let text = "!きしゃ\t記者\t名詞\n#きしゃ\t汽車\t名詞\n";
    let ms = read_ime_dictionary(text, ImeFormat::MsIme).unwrap();
    assert_eq!(ms.words.len(), 1);
    let google = read_ime_dictionary(text, ImeFormat::Google).unwrap();
    assert_eq!(google.words.len(), 1);
    // `!` and `#` are not readings Kanaemi can type... unless made full-width.
    assert_eq!(lines_of(&ms), ["＃きしゃ\t汽車"]);
    assert_eq!(lines_of(&google), ["！きしゃ\t記者"]);
}

#[test]
fn atok_marks_after_the_part_of_speech_are_dropped() {
    let file = utf16le(
        "!!ATOK_TANGO_TEXT_HEADER_1\r\n!一覧出力\r\nかく\t書く\tカ行五段*\r\nかな\t仮名\t名詞$\r\n",
    );
    let read = read_ime_dictionary(file, ImeFormat::Atok).unwrap();
    assert_eq!(lines_of(&read), ["か\t書\t五段-カ行", "かな\t仮名"]);
}

#[test]
fn full_width_and_half_width_parts_of_speech_are_the_same() {
    let read = read_ime_dictionary(
        "たかい\t高い\t形容詞ｶﾞﾙ\nかく\t書く\t動詞カ行五段\n",
        ImeFormat::Google,
    )
    .unwrap();
    assert_eq!(lines_of(&read), ["たか\t高\t形容詞", "か\t書\t五段-カ行"]);
}

#[test]
fn encodings_are_told_by_the_bom_then_by_the_bytes() {
    let text = "きしゃ\t記者\t名詞\r\n";
    let (sjis, _, _) = SHIFT_JIS.encode(text);
    let (euc, _, _) = encoding_rs::EUC_JP.encode(text);
    let mut utf16be = vec![0xFE, 0xFF];
    utf16be.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
    let utf16le_without_bom = utf16le(text)[2..].to_vec();
    let utf16be_without_bom = utf16be[2..].to_vec();
    let mut utf8_with_bom = b"\xEF\xBB\xBF".to_vec();
    utf8_with_bom.extend(text.as_bytes());
    for bytes in [
        utf16le(text),
        utf16be.clone(),
        utf16le_without_bom,
        utf16be_without_bom,
        utf8_with_bom,
        text.as_bytes().to_vec(),
        sjis.into_owned(),
    ] {
        let read = read_ime_dictionary(&bytes, ImeFormat::MsIme).unwrap();
        assert_eq!(lines_of(&read), ["きしゃ\t記者"], "{bytes:x?}");
    }
    // EUC-JP is no encoding these files come in: as Shift_JIS it is either
    // other characters or fails.
    assert_ne!(
        read_ime_dictionary(euc.as_ref(), ImeFormat::MsIme).map(|r| lines_of(&r).join("")),
        Ok("きしゃ\t記者".to_owned())
    );
}

#[test]
fn bytes_the_encoding_cannot_decode_fail_the_import_at_their_line() {
    let mut bytes = "きしゃ\t記者\t名詞\r\n".as_bytes().to_vec();
    let (mut sjis, _, _) = SHIFT_JIS.encode("きしゃ\t記者\t名詞\r\n");
    let sjis = sjis.to_mut();
    // A lead byte with nothing after it.
    sjis.extend_from_slice(b"\x82");
    bytes.clear();
    bytes.extend_from_slice(sjis);
    assert_eq!(
        read_ime_dictionary(&bytes, ImeFormat::MsIme),
        Err(ImeDictionaryError::Undecodable {
            encoding: "Shift_JIS",
            line: 2,
        })
    );
    // An odd byte at the end of UTF-16.
    let mut odd = utf16le("きしゃ\t記者\t名詞\r\n");
    odd.push(0x41);
    assert!(matches!(
        read_ime_dictionary(&odd, ImeFormat::MsIme),
        Err(ImeDictionaryError::Undecodable {
            encoding: "UTF-16LE",
            ..
        })
    ));
}

const PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<array>
	<dict>
		<key>phrase</key>
		<string>記者</string>
		<key>shortcut</key>
		<string>きしゃ</string>
	</dict>
	<dict>
		<key>phrase</key>
		<string>On my way!</string>
		<key>shortcut</key>
		<string>omw</string>
	</dict>
	<dict>
		<key>phrase</key>
		<string>一行目
二行目</string>
		<key>shortcut</key>
		<string>にぎょう</string>
	</dict>
	<dict>
		<key>phrase</key>
		<string>読みなし</string>
	</dict>
</array>
</plist>
"#;

#[test]
fn a_macos_property_list_is_read_by_shortcut_and_phrase() {
    let read = read_ime_dictionary(PLIST, ImeFormat::MacOs).unwrap();
    assert_eq!(
        lines_of(&read),
        ["きしゃ\t記者", "にぎょう\t一行目\\n二行目"]
    );
    assert_eq!(
        reasons(&read),
        [
            skipped(2, SkipReason::Unrepresentable),
            skipped(4, SkipReason::Unreadable)
        ]
    );
    let d = custom(&lines_of(&read).join("\n"));
    assert_eq!(d.lookup("にぎょう")[0].surface, "一行目\n二行目");
}

#[test]
fn a_file_that_is_not_a_property_list_of_words_fails() {
    assert!(matches!(
        read_ime_dictionary("きしゃ\t記者\n", ImeFormat::MacOs),
        Err(ImeDictionaryError::NotPropertyList(_))
    ));
    let dict = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>a</key><string>b</string></dict></plist>"#;
    assert_eq!(
        read_ime_dictionary(dict, ImeFormat::MacOs),
        Err(ImeDictionaryError::NotArray)
    );
}

#[test]
fn a_binary_property_list_is_read_too() {
    let mut item = plist::Dictionary::new();
    item.insert("shortcut".into(), plist::Value::String("きしゃ".into()));
    item.insert("phrase".into(), plist::Value::String("記者".into()));
    let mut bytes = Vec::new();
    plist::Value::Array(vec![plist::Value::Dictionary(item)])
        .to_writer_binary(&mut bytes)
        .unwrap();
    let read = read_ime_dictionary(bytes, ImeFormat::MacOs).unwrap();
    assert_eq!(lines_of(&read), ["きしゃ\t記者"]);
}

fn word(line: usize, lines: &[&str]) -> ImeWord {
    ImeWord {
        line,
        lines: lines.iter().map(|l| (*l).to_owned()).collect(),
    }
}

#[test]
fn the_words_read_make_a_text_dictionary_with_each_line_once() {
    let read = ImeWords {
        words: vec![
            word(1, &["きしゃ\t記者"]),
            word(2, &["べんきょう\t勉強", "べんきょう\t勉強\tサ行変格"]),
            word(3, &["きしゃ\t記者"]),
        ],
        skipped: Vec::new(),
    };
    assert_eq!(
        read.text(),
        "きしゃ\t記者\nべんきょう\t勉強\nべんきょう\t勉強\tサ行変格\n"
    );
}

const CUSTOM: &str = "\
きしゃ\t記者
きしゃ\t汽車
か\t書\t五段-カ行
い\t行\t五段-カ行-促音便
たべ\t食べ\t下一段-バ行
たか\t高\t形容詞
べんきょう\t勉強
べんきょう\t勉強\tサ行変格
か*っ\t勝っ
か*k\t書
{}こ\t{}個
きょう\t{-:date %Y-%m-%d}
た\tた\t助動詞-タ
!ねこ\t猫
";

#[test]
fn a_user_custom_dictionary_is_written_for_google_japanese_input() {
    let export = write_ime_dictionary(&custom(CUSTOM), ImeFormat::Google);
    assert_eq!(
        String::from_utf8(export.bytes).unwrap(),
        "いく\t行く\t動詞カ行五段\n\
         かく\t書く\t動詞カ行五段\n\
         かっ\t勝っ\t名詞\n\
         きしゃ\t汽車\t名詞\n\
         きしゃ\t記者\t名詞\n\
         たかい\t高い\t形容詞\n\
         たべる\t食べる\t動詞一段\n\
         ねこ\t猫\t抑制単語\n\
         べんきょう\t勉強\t名詞サ変\n"
    );
    assert_eq!(export.written, 9);
    // か*k, {}こ, the function, and 助動詞-タ.
    assert_eq!(export.skipped, 4);
}

#[test]
fn ms_ime_and_atok_get_utf_16_with_their_headers() {
    let d = custom("かく\t書く\nか\t書\t五段-カ行\nと\t問\t五段-ワア行\n!ねこ\t猫\n");
    let ms = write_ime_dictionary(&d, ImeFormat::MsIme);
    assert_eq!(&ms.bytes[..2], [0xFF, 0xFE]);
    let (text, _) = UTF_16LE.decode_without_bom_handling(&ms.bytes[2..]);
    assert_eq!(
        text,
        "!Microsoft IME Dictionary Tool\r\n\
         かく\t書く\tか行五段\r\n\
         かく\t書く\t名詞\r\n\
         とう\t問う\tあわ行五段\r\n\
         ねこ\t猫\t抑制単語\r\n"
    );
    let atok = write_ime_dictionary(&d, ImeFormat::Atok);
    let (text, _) = UTF_16LE.decode_without_bom_handling(&atok.bytes[2..]);
    assert_eq!(
        text,
        "!!ATOK_TANGO_TEXT_HEADER_1\r\n\
         かく\t書く\tカ行五段\r\n\
         かく\t書く\t名詞\r\n\
         とう\t問う\tワ行五段\r\n"
    );
    assert_eq!((atok.written, atok.skipped), (3, 1), "ATOK has no 抑制単語");
}

#[test]
fn words_a_text_format_cannot_hold_are_left_out() {
    let d = custom("\\!a\tb\n\\#a\tb\nにぎょう\t一\\n二\nたぶ\ta\\tb\n");
    let ms = write_ime_dictionary(&d, ImeFormat::MsIme);
    assert_eq!((ms.written, ms.skipped), (1, 3));
    let google = write_ime_dictionary(&d, ImeFormat::Google);
    assert_eq!((google.written, google.skipped), (1, 3));
    let mac = write_ime_dictionary(&d, ImeFormat::MacOs);
    assert_eq!(mac.skipped, 0);
}

#[test]
fn a_property_list_is_written_for_macos() {
    let d = custom("きしゃ\t記者\nにぎょう\t一\\n<二>\n!ねこ\t猫\n");
    let export = write_ime_dictionary(&d, ImeFormat::MacOs);
    let text = String::from_utf8(export.bytes.clone()).unwrap();
    assert!(text.starts_with("<?xml"), "{text}");
    assert!(text.contains("<key>phrase</key>"), "{text}");
    assert!(text.contains("&lt;二&gt;"), "{text}");
    assert_eq!((export.written, export.skipped), (2, 1));
    let read = read_ime_dictionary(&export.bytes, ImeFormat::MacOs).unwrap();
    assert_eq!(lines_of(&read), ["きしゃ\t記者", "にぎょう\t一\\n<二>"]);
}

#[test]
fn what_is_written_out_comes_back_in() {
    let kept = "\
きしゃ\t記者
か\t書\t五段-カ行
い\t行\t五段-カ行-促音便
たべ\t食べ\t下一段-バ行
み\t見\t上一段-マ行
たか\t高\t形容詞
べんきょう\t勉強
べんきょう\t勉強\tサ行変格
";
    let original = custom(kept);
    for format in [ImeFormat::MsIme, ImeFormat::Google] {
        let export = write_ime_dictionary(&original, format);
        let read = read_ime_dictionary(&export.bytes, format).unwrap();
        let mut back: Vec<&str> = lines_of(&read);
        back.sort_unstable();
        let mut lines: Vec<&str> = kept.lines().collect();
        lines.sort_unstable();
        assert_eq!(back, lines, "{format:?}");
    }
}
