use std::process::Command;

use kanaemi_core::VERSION;

fn git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8(output.stdout).unwrap().trim().to_owned())
}

#[test]
fn the_version_is_the_latest_release_tag_described_without_its_v() {
    if let Some(given) = option_env!("KANAEMI_BUILD_VERSION").filter(|v| !v.is_empty()) {
        assert_eq!(VERSION, given);
        return;
    }
    // Outside this repository's own checkout, any enclosing repository's tags
    // say nothing about this crate, so only the version Cargo records counts.
    let in_own_checkout = git(&["rev-parse", "--show-prefix"]).as_deref() == Some("crates/core/");
    let described = in_own_checkout
        .then(|| git(&["describe", "--tags", "--match", "v[0-9]*"]))
        .flatten();
    let expected = match &described {
        Some(described) => described.strip_prefix('v').unwrap(),
        None => env!("CARGO_PKG_VERSION"),
    };
    assert_eq!(VERSION, expected);
}
