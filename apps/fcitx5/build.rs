//! Builds the C++ layer Fcitx5 loads, against the Fcitx5 found by
//! pkg-config. Where Fcitx5 does not run, the library holds only what is
//! built and tested on every platform.

use std::env;

fn main() {
    println!("cargo::rerun-if-changed=src/addon.cpp");
    let unix = env::var("CARGO_CFG_TARGET_FAMILY").is_ok_and(|family| family == "unix");
    let macos = env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "macos");
    if !unix || macos {
        return;
    }
    let fcitx = match pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("Fcitx5Core")
    {
        Ok(fcitx) => fcitx,
        Err(error) => panic!("the add-on is built against Fcitx5's development files: {error}"),
    };
    let mut build = cc::Build::new();
    build
        .cpp(true)
        // Newer Fcitx5 headers use std::span and std::ranges.
        .std("c++20")
        .file("src/addon.cpp")
        .includes(&fcitx.include_paths);
    if at_least(&fcitx.version, [5, 1, 9]) {
        build.define("KANAEMI_FCITX5_CANDIDATE_COMMENT", None);
    }
    build.compile("kanaemi_fcitx5_addon");
    // Named after the C++ layer that calls them: named before, as pkg-config
    // would, the linker finds nothing needs them yet and leaves them out, and
    // the add-on would not say which Fcitx5 it was built for.
    for path in &fcitx.link_paths {
        println!("cargo::rustc-link-search=native={}", path.display());
    }
    for lib in &fcitx.libs {
        println!("cargo::rustc-link-lib={lib}");
    }
}

/// Whether a dotted version is `wanted` or later. A part that is not a number
/// counts as 0.
fn at_least(version: &str, wanted: [u32; 3]) -> bool {
    let mut parts = version.split('.').map(|part| part.parse().unwrap_or(0));
    let found: [u32; 3] = std::array::from_fn(|_| parts.next().unwrap_or(0));
    found >= wanted
}
