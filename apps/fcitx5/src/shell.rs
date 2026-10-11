//! Everything the add-on keeps between events: the profile every field
//! shares and a field for each input context. Fcitx5 runs add-ons on its
//! one event loop, so all of it lives on that thread.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use kanaemi_core::Event;
use kanaemi_linux::keys::{Keys, RELEASE_MASK};
use kanaemi_linux::reply::Reply;
use kanaemi_runtime::{Field, Profile};

use crate::content::content;
use crate::repeat;
use crate::show::Erasing;

/// An input context, as the C++ layer names it.
pub(crate) type Id = usize;

struct Context {
    field: Field,
    password: bool,
    erasing: Erasing,
}

pub(crate) struct Shell {
    pub profile: Profile,
    contexts: HashMap<Id, Context>,
    focused: Option<Id>,
    /// The keyboard's keys down, whichever context has the focus.
    keys: Keys,
    started: Instant,
}

impl Shell {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            profile: Profile::open(dir),
            contexts: HashMap::new(),
            focused: None,
            keys: Keys::default(),
            started: Instant::now(),
        }
    }

    pub fn create(&mut self, id: Id) {
        let context = Context {
            field: Field::new(&self.profile),
            password: false,
            erasing: Erasing::ByKeys,
        };
        self.contexts.insert(id, context);
    }

    /// Forgets a context Fcitx5 destroyed. One may go with the focus still
    /// in it; other programs are told no field has it.
    pub fn destroy(&mut self, id: Id) {
        if self.focused == Some(id) {
            self.focus_out(id, false);
        }
        self.contexts.remove(&id);
    }

    fn handle(&mut self, id: Id, event: Event) -> Reply {
        match self.contexts.get_mut(&id) {
            // Fcitx5 draws its own panel for the candidates and the text
            // beside them, which shows the mode by the caret well.
            Some(context) => {
                kanaemi_linux::handle(&mut context.field, &mut self.profile, event, true)
            }
            None => Reply::NOTHING,
        }
    }

    pub fn key(&mut self, id: Id, keysym: u32, code: u32, state: u32, release: bool) -> Reply {
        let time_ms = self.started.elapsed().as_millis() as u64;
        let state = if release {
            state | RELEASE_MASK
        } else {
            state & !RELEASE_MASK
        };
        match self.keys.translate(keysym, code, state, time_ms) {
            Some(key) => self.handle(id, Event::Key(repeat::marked(key, state))),
            None => Reply::NOTHING,
        }
    }

    /// The focus comes into a field of `program`, with the capability
    /// flags Fcitx5 holds for it. Where a key forwarded to the field carries
    /// the modifiers held down rather than its own (`held_modifiers`), no
    /// key is sent in place of another, and text is erased from the text
    /// around the caret.
    pub fn focus_in(&mut self, id: Id, flags: u64, program: &str, held_modifiers: bool) -> Reply {
        let Some(context) = self.contexts.get_mut(&id) else {
            return Reply::NOTHING;
        };
        let (password, private) = content(flags);
        context.password = password;
        context.erasing = if held_modifiers {
            Erasing::AroundTheCaret
        } else {
            Erasing::ByKeys
        };
        context.field.set_private(private);
        context.field.set_application(program);
        context.field.set_sends_keys(!held_modifiers);
        self.focused = Some(id);
        self.handle(id, Event::FocusIn { password })
    }

    /// Takes the field's capability flags again. A client may tell them
    /// after the focus came in, so a field that turns out to be a password
    /// one is focused again with that known.
    pub fn set_capabilities(&mut self, id: Id, flags: u64) -> Option<Reply> {
        let (password, private) = content(flags);
        let context = self.contexts.get_mut(&id)?;
        context.field.set_private(private);
        let changed = std::mem::replace(&mut context.password, password) != password;
        (changed && self.focused == Some(id)).then(|| self.handle(id, Event::FocusIn { password }))
    }

    /// The focus leaves the field, or another input method takes it over.
    /// Each field has a context of its own, so what is typed is committed
    /// to the one it was typed in, where the field can still take it
    /// (`committable`); elsewhere it is dropped.
    pub fn focus_out(&mut self, id: Id, committable: bool) -> Reply {
        if self.focused == Some(id) {
            self.focused = None;
        }
        // Keys let go while another input method has them never reach here;
        // one still taken for down would make the next press its repeat.
        self.keys = Keys::default();
        if committable {
            return self.handle(id, Event::FocusOut);
        }
        if let Some(context) = self.contexts.get_mut(&id) {
            kanaemi_linux::drop_focus(&mut context.field, &mut self.profile);
        }
        Reply::NOTHING
    }

    /// How text before the caret is erased in the context.
    pub fn erasing(&self, id: Id) -> Erasing {
        self.contexts
            .get(&id)
            .map_or(Erasing::ByKeys, |context| context.erasing)
    }

    /// The application asks for the preedit to be committed, as on a click.
    pub fn reset(&mut self, id: Id) -> Reply {
        self.handle(id, Event::Flush)
    }

    pub fn select(&mut self, id: Id, index: usize) -> Reply {
        self.handle(id, Event::Select(index))
    }

    /// Answers the requests other programs sent; a mode to set goes to the
    /// field with the focus first, which is told what came of it.
    pub fn serve_control(&mut self, mut tell: impl FnMut(Id, Reply, Erasing)) {
        for request in self.profile.take_control_requests() {
            if let Some(mode) = request.mode_to_set()
                && let Some(id) = self.focused
            {
                let reply = self.handle(id, Event::SetMode(mode));
                tell(id, reply, self.erasing(id));
            }
            self.profile.answer(request);
        }
    }
}
