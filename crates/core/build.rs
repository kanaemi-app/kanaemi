use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // A build without the repository, such as Nix's, says what it is.
    println!("cargo:rerun-if-env-changed=KANAEMI_BUILD_VERSION");
    if let Some(version) = std::env::var("KANAEMI_BUILD_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
    {
        println!("cargo:rustc-env=KANAEMI_VERSION={version}");
        return;
    }
    // Built from a published or vendored copy, the crate can still sit inside
    // some other repository (a home directory under Git, a project that
    // vendors its dependencies), whose tags say nothing about this crate.
    if git(&["rev-parse", "--show-prefix"]).as_deref() != Some("crates/core/") {
        let version = std::env::var("CARGO_PKG_VERSION").unwrap();
        println!("cargo:rustc-env=KANAEMI_VERSION={version}");
        return;
    }

    let version = git(&["describe", "--tags", "--match", "v[0-9]*"])
        .and_then(|described| described.strip_prefix('v').map(str::to_owned))
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=KANAEMI_VERSION={version}");

    // A tag can be added without touching any source file, so the Git state
    // that `git describe` reads has to be watched here; Cargo would otherwise
    // keep the version described at the first build.
    let git_dir = git(&["rev-parse", "--absolute-git-dir"]);
    let common_dir = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    let watched =
        git_dir
            .iter()
            .map(|dir| Path::new(dir).join("HEAD"))
            .chain(common_dir.iter().flat_map(|dir| {
                // `reftable` holds every ref in a repository using that format.
                ["refs/heads", "refs/tags", "packed-refs", "reftable"]
                    .map(|path| Path::new(dir).join(path))
            }));
    // A path that does not exist would make Cargo run this script every build.
    for path in watched.filter(|path| path.exists()) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}
