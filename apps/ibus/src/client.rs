//! What IBus tells of the client an engine serves, read apart from D-Bus so
//! it builds and tests on every platform.

/// The input purposes of a password and a PIN, as IBus numbers them.
const SECRET_PURPOSES: [u32; 2] = [8, 9];
/// The hint of a field whose text is not to be remembered, such as one in a
/// browser's private window.
const PRIVATE_HINT: u32 = 1 << 11;

/// Whether the panel of `desktop` (`XDG_CURRENT_DESKTOP`) shows the mode by
/// the caret well: GNOME Shell does. IBus's own panel, on other desktops,
/// keeps an empty frame of candidates up after the text goes, so there the
/// mode is not shown at all.
pub fn shows_indicator(desktop: &str) -> bool {
    desktop.split(':').any(|d| d == "GNOME")
}

/// What a field's content type says: whether it takes a secret, and
/// whether it asks that what is typed there not be recorded.
pub fn content_type(purpose: u32, hints: u32) -> (bool, bool) {
    (
        SECRET_PURPOSES.contains(&purpose),
        hints & PRIVATE_HINT != 0,
    )
}

/// The program an input context's client (`FocusInId`) is, when it says:
/// GTK's input modules append it (`gtk4-im:ghostty`), and XIM or GNOME
/// Shell's own entries tell none.
pub fn program(client: &str) -> Option<&str> {
    match client.split_once(':') {
        Some(("gtk-im" | "gtk3-im" | "gtk4-im", program)) => Some(program),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_shows_the_mode_by_the_caret_and_other_panels_do_not() {
        assert!(shows_indicator("GNOME"));
        assert!(shows_indicator("ubuntu:GNOME"));
        assert!(!shows_indicator("KDE"));
        assert!(!shows_indicator(""));
    }

    #[test]
    fn a_password_or_pin_is_a_secret_and_the_private_hint_asks_for_no_record() {
        assert_eq!(content_type(8, 0), (true, false));
        assert_eq!(content_type(9, 0), (true, false));
        assert_eq!(content_type(0, 1 << 11), (false, true));
        assert_eq!(content_type(0, 0), (false, false));
    }

    #[test]
    fn a_gtk_client_names_its_program_and_others_name_none() {
        assert_eq!(program("gtk4-im:ghostty"), Some("ghostty"));
        assert_eq!(
            program("gtk3-im:gnome-terminal-server"),
            Some("gnome-terminal-server")
        );
        assert_eq!(program("gtk-im:firefox"), Some("firefox"));
        assert_eq!(program("xim"), None);
        assert_eq!(program("gnome-shell"), None);
        assert_eq!(program("fake"), None);
    }
}
