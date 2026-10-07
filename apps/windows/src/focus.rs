//! Which context of the focused document has the focus, as applications
//! push contexts onto its stack and pop them off.

/// How the focus goes once a context is pushed or popped.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Change<C> {
    /// The context with the focus keeps it.
    Stays,
    /// The focus comes into the field at `C`, where none had it.
    Comes(C),
    /// The focus leaves the field at `C` for none.
    Goes(C),
    /// The field goes on from one context to the other. Covering a document
    /// with a context of its own for a while, or uncovering it, does not
    /// take the user to another field.
    Passes { from: C, to: C },
}

/// How the focus goes from `held`, the context that had it, once `popped`,
/// if any, is off the stack: `stack` is the focused document's, top first,
/// read after the change. It may still hold `popped`, as it may be told
/// before the context goes.
pub(crate) fn change<C: PartialEq>(
    stack: impl IntoIterator<Item = C>,
    popped: Option<&C>,
    held: Option<C>,
) -> Change<C> {
    match (held, top_after(stack, popped)) {
        (held, top) if held == top => Change::Stays,
        (None, Some(top)) => Change::Comes(top),
        (Some(held), None) => Change::Goes(held),
        (Some(from), Some(to)) => Change::Passes { from, to },
        (None, None) => Change::Stays,
    }
}

/// The context with the focus once `popped`, if any, is off the stack.
fn top_after<C: PartialEq>(stack: impl IntoIterator<Item = C>, popped: Option<&C>) -> Option<C> {
    stack.into_iter().find(|context| Some(context) != popped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pushed_context_takes_over_the_field() {
        assert_eq!(
            change(["pushed", "base"], None, Some("base")),
            Change::Passes {
                from: "base",
                to: "pushed"
            }
        );
    }

    #[test]
    fn a_popped_context_still_on_the_stack_hands_the_field_back() {
        assert_eq!(
            change(["pushed", "base"], Some(&"pushed"), Some("pushed")),
            Change::Passes {
                from: "pushed",
                to: "base"
            }
        );
    }

    #[test]
    fn a_popped_context_already_gone_hands_the_field_back() {
        assert_eq!(
            change(["base"], Some(&"pushed"), Some("pushed")),
            Change::Passes {
                from: "pushed",
                to: "base"
            }
        );
    }

    #[test]
    fn a_context_popped_elsewhere_leaves_the_focus_where_it_is() {
        assert_eq!(
            change(["base"], Some(&"other"), Some("base")),
            Change::Stays
        );
    }

    #[test]
    fn the_last_context_popped_takes_the_focus_away() {
        assert_eq!(
            change(["base"], Some(&"base"), Some("base")),
            Change::Goes("base")
        );
    }

    #[test]
    fn a_context_pushed_where_none_had_the_focus_takes_it() {
        assert_eq!(change(["pushed"], None, None), Change::Comes("pushed"));
    }

    #[test]
    fn no_focused_document_has_no_focus() {
        assert_eq!(change(Vec::<&str>::new(), None, None), Change::Stays);
    }
}
