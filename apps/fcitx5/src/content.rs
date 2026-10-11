//! What Fcitx5's capability flags tell of a field.

/// A field whose text is hidden, as a password's or a PIN's.
const PASSWORD: u64 = 1 << 3;
/// A field that holds sensitive data, such as a card number, which is not
/// to be remembered.
const SENSITIVE: u64 = 1 << 36;

/// Whether the field takes a secret, and whether it asks that what is typed
/// there not be recorded.
pub(crate) fn content(flags: u64) -> (bool, bool) {
    (flags & PASSWORD != 0, flags & SENSITIVE != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_is_a_secret_and_sensitive_data_asks_for_no_record() {
        assert_eq!(content(1 << 3), (true, false));
        assert_eq!(content(1 << 36), (false, true));
        assert_eq!(content((1 << 3) | (1 << 8)), (true, false), "a PIN");
        assert_eq!(content(1 << 1), (false, false));
    }
}
