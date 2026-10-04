//! The settings app.
//!
//! A program of its own rather than a window of the input method, so the
//! input method stays a background process that only handles keys.

use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        main {
            h1 { "Kanaemi" }
            p { "Version {kanaemi_core::VERSION}" }
        }
    }
}
