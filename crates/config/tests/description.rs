use kanaemi_config::description;

#[test]
fn the_first_line_comment_describes_the_file() {
    assert_eq!(
        description("# ヘボン式\nshi\tし\n").as_deref(),
        Some("ヘボン式")
    );
    assert_eq!(
        description("\u{feff}#訓令式\r\nsi\tし").as_deref(),
        Some("訓令式")
    );
}

#[test]
fn a_file_without_a_leading_comment_has_no_description() {
    assert_eq!(description("shi\tし\n# later\n"), None);
    assert_eq!(description("#\nshi\tし\n"), None);
    assert_eq!(description(""), None);
}
