//! Line icons from Tabler Icons 3.48.0 (MIT, https://tabler.io/icons),
//! drawn in the current text colour.

use dioxus::prelude::*;

pub const EYE: &str = r#"<path d="M10 12a2 2 0 1 0 4 0a2 2 0 0 0 -4 0"/><path d="M21 12c-2.4 4 -5.4 6 -9 6c-3.6 0 -6.6 -2 -9 -6c2.4 -4 5.4 -6 9 -6c3.6 0 6.6 2 9 6"/>"#;
pub const KEYBOARD: &str = r#"<path d="M2 8a2 2 0 0 1 2 -2h16a2 2 0 0 1 2 2v8a2 2 0 0 1 -2 2h-16a2 2 0 0 1 -2 -2l0 -8"/><path d="M6 10l0 .01"/><path d="M10 10l0 .01"/><path d="M14 10l0 .01"/><path d="M18 10l0 .01"/><path d="M6 14l0 .01"/><path d="M18 14l0 .01"/><path d="M10 14l4 .01"/>"#;
pub const BOOK: &str = r#"<path d="M3 19a9 9 0 0 1 9 0a9 9 0 0 1 9 0"/><path d="M3 6a9 9 0 0 1 9 0a9 9 0 0 1 9 0"/><path d="M3 6l0 13"/><path d="M12 6l0 13"/><path d="M21 6l0 13"/>"#;
pub const COMMAND: &str =
    r#"<path d="M7 9a2 2 0 1 1 2 -2v10a2 2 0 1 1 -2 -2h10a2 2 0 1 1 -2 2v-10a2 2 0 1 1 2 2h-10"/>"#;
pub const INFO_CIRCLE: &str = r#"<path d="M3 12a9 9 0 1 0 18 0a9 9 0 0 0 -18 0"/><path d="M12 9h.01"/><path d="M11 12h1v4h1"/>"#;
pub const GRIP_VERTICAL: &str = r#"<path d="M8 5a1 1 0 1 0 2 0a1 1 0 1 0 -2 0"/><path d="M8 12a1 1 0 1 0 2 0a1 1 0 1 0 -2 0"/><path d="M8 19a1 1 0 1 0 2 0a1 1 0 1 0 -2 0"/><path d="M14 5a1 1 0 1 0 2 0a1 1 0 1 0 -2 0"/><path d="M14 12a1 1 0 1 0 2 0a1 1 0 1 0 -2 0"/><path d="M14 19a1 1 0 1 0 2 0a1 1 0 1 0 -2 0"/>"#;
pub const ARROW_RIGHT: &str =
    r#"<path d="M5 12l14 0"/><path d="M13 18l6 -6"/><path d="M13 6l6 6"/>"#;
pub const PLUS: &str = r#"<path d="M12 5l0 14"/><path d="M5 12l14 0"/>"#;
pub const X: &str = r#"<path d="M18 6l-12 12"/><path d="M6 6l12 12"/>"#;
pub const FOLDER_OPEN: &str = r#"<path d="M5 19l2.757 -7.351a1 1 0 0 1 .936 -.649h12.307a1 1 0 0 1 .986 1.164l-.996 5.211a2 2 0 0 1 -1.964 1.625h-14.026a2 2 0 0 1 -2 -2v-11a2 2 0 0 1 2 -2h4l3 3h7a2 2 0 0 1 2 2v2"/>"#;
pub const CODE: &str =
    r#"<path d="M7 8l-4 4l4 4"/><path d="M17 8l4 4l-4 4"/><path d="M14 4l-4 16"/>"#;
pub const SELECTOR: &str = r#"<path d="M8 9l4 -4l4 4"/><path d="M16 15l-4 4l-4 -4"/>"#;
pub const CHECK: &str = r#"<path d="M5 12l5 5l10 -10"/>"#;
pub const LIST_SEARCH: &str = r#"<path d="M11 15a4 4 0 1 0 8 0a4 4 0 1 0 -8 0"/><path d="M18.5 18.5l2.5 2.5"/><path d="M4 6h16"/><path d="M4 12h4"/><path d="M4 18h4"/>"#;

/// One of the icons above.
#[component]
pub fn Icon(paths: &'static str) -> Element {
    rsx! {
        svg {
            class: "icon",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            dangerous_inner_html: paths,
        }
    }
}
