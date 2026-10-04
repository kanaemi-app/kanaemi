//! COM's exports are found by name and must stay out of the import library,
//! which only a module definition file can say.

fn main() {
    println!("cargo:rerun-if-changed=kanaemi.def");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let def = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("kanaemi.def");
        println!("cargo:rustc-cdylib-link-arg=/DEF:{}", def.display());
    }
}
