use std::mem;

use crate::edit::{CursorMove, Editable};
use crate::word::Word;
use crate::{
    Action, Candidate, CandidateView, Chord, Config, Converter, Effect, Event, Form, Gesture, Key,
    KeyEvent, KeyKind, Mode, Modifiers, Output, PAGE_LEN, Scene, romaji,
};

const MAX_REGISTRATION_DEPTH: usize = 3;

#[derive(Clone)]
enum State {
    Idle { pending: String },
    Reading(Word),
    Candidates(Selection),
}

impl State {
    fn idle() -> Self {
        Self::Idle {
            pending: String::new(),
        }
    }
}

#[derive(Clone)]
struct Selection {
    word: Word,
    /// Never empty: the reading's own forms cannot be forgotten.
    candidates: Vec<Candidate>,
    /// How many candidates, from the first, came from the converter; the
    /// rest are the reading's own forms.
    converted: usize,
    index: usize,
    /// Romaji typed past the okurigana's first kana, carried to the next word.
    rest: String,
    /// Keys typed while choosing that go on with `rest` but finish no kana
    /// yet (the `s` of `;i;tts`), carried with it.
    typed: String,
    /// `forget` was pressed once: pressed again, it forgets the selected
    /// candidate. Any other key, a lone modifier aside, withdraws it.
    forgetting: bool,
    /// The completion the word was converted from, to go on with when the
    /// keys that complete are pressed while choosing.
    completion: Option<Box<Completion>>,
}

/// Shown after the selected candidate while asking whether to forget it.
const ASKING_TO_FORGET: &str = "（候補から除外するには、もう一度同じキーを押してください）";

impl Selection {
    /// Everything typed after the okurigana's first kana.
    fn after(&self) -> String {
        format!("{}{}", self.rest, self.typed)
    }

    /// The word as typed, its romaji left over from the okurigana back on it.
    /// What was typed while choosing is not part of the word, and is dropped.
    fn into_reading(self) -> Word {
        let mut word = self.word;
        word.pending = self.rest;
        word
    }
}

/// A candidate committed, as it was chosen, and the text the core put in the
/// field after it since. Only what the core itself committed is known to be
/// before the caret, so any key passed to the application ends it.
#[derive(Clone)]
struct Undoable {
    /// Romaji left after the okurigana went into `after` as it was typed.
    selection: Selection,
    surface: String,
    after: String,
}

impl Undoable {
    fn committed(&self) -> Effect {
        Effect::Committed {
            reading: self.selection.word.kana(),
            okurigana: self.selection.word.okurigana().map(str::to_owned),
            surface: self.surface.clone(),
        }
    }

    fn withdrawn(&self) -> Effect {
        Effect::Withdrawn {
            reading: self.selection.word.kana(),
            okurigana: self.selection.word.okurigana().map(str::to_owned),
            surface: self.surface.clone(),
        }
    }

    /// What it put in the field.
    fn text(&self) -> String {
        format!("{}{}", self.surface, self.after)
    }
}

/// The readings a reading is completed with, gone round while only the
/// keys that complete are pressed.
#[derive(Clone)]
struct Completion {
    /// The word as typed, its romaji made kana.
    typed: Word,
    /// Never empty; each longer than the typed reading and starting with it.
    readings: Vec<String>,
    /// The candidates of each reading, beside `readings`: `None` until a page
    /// it is on is shown, then those the converter gave, and how many were
    /// asked for.
    previews: Vec<Option<(Vec<String>, usize)>>,
    /// The reading shown; `None` for the word as typed.
    index: Option<usize>,
}

/// How many candidates of the highlighted reading to complete with are
/// shown: its first, and after it as many as a page's others.
const PREVIEW_LEN: usize = PAGE_LEN;

impl Completion {
    /// Asks `converter` for the first candidate of each reading on the page
    /// `index` is on, and for more of the reading at `index`; each once.
    fn preview(&mut self, converter: &impl Converter, index: usize) {
        let start = page_start(index);
        let end = (start + PAGE_LEN).min(self.readings.len());
        for at in start..end {
            let wanted = if at == index { PREVIEW_LEN } else { 1 };
            if self.previews[at]
                .as_ref()
                .is_none_or(|(_, asked)| *asked < wanted)
            {
                let candidates = converter.preview(&self.readings[at], wanted);
                self.previews[at] = Some((candidates, wanted));
            }
        }
    }
}

/// What the host is asked to erase before the core goes on.
#[derive(Clone)]
enum Erasing {
    /// A commit undone, to choose again.
    Commit(Undoable),
    /// Kana typed without a reading, to read again: the kana and the keys
    /// that made them, with the romaji still unfinished after them.
    Kana(Word),
}

#[derive(Clone)]
struct Registration {
    word: Word,
    text: Editable,
}

/// A key bound to be held, from its press until it is let go.
#[derive(Clone)]
struct Held {
    pressed: Chord,
    at: u64,
    state: HeldState,
}

#[derive(Clone, Copy)]
enum HeldState {
    /// Neither held nor pressed alone yet; a character typed meanwhile waits.
    Undecided(Option<Chord>),
    /// Each character typed acts the binding first.
    Holding,
    /// Already acted as pressed alone; the rest of the press is nothing.
    Alone,
}

/// The input method: events in, the state to show out.
///
/// A copy tries an event without changing the original, as a host that is
/// asked whether a key will be used before it is sent needs.
#[derive(Clone)]
pub struct Core<C> {
    converter: C,
    config: Config,
    mode: Mode,
    password: bool,
    /// Whether the keys of [`crate::Bindings::application`] are sent in the
    /// application with the focus.
    sending: bool,
    state: State,
    /// Outermost first.
    registrations: Vec<Registration>,
    /// The modifier keys that can be tapped and are down now.
    modifiers_held: Vec<Key>,
    modifier_down: Option<(Key, u64)>,
    held: Option<Held>,
    /// The reading being completed, while the keys that complete are
    /// pressed one after another.
    completion: Option<Completion>,
    /// The last candidate committed, while it can be undone.
    undoable: Option<Undoable>,
    /// A commit undone or kana to read again, waiting for the host to erase it.
    erasing: Option<Erasing>,
    /// Keys that came while the host was erasing.
    keys_waiting: Vec<KeyEvent>,
    /// A commit undone and being chosen again: its `after` follows whatever
    /// is committed next, and Cancel commits it as it was.
    redoing: Option<Undoable>,
    /// The kana last typed in kana mode with nothing typed, one run of them
    /// since the core last put anything else in the field, with the keys
    /// that made them and the romaji still unfinished: what a reading
    /// forgotten to begin is taken back from. Only kana, so that what is
    /// read again can be converted.
    kana_run: Option<Word>,
    /// Kana taken back to the reading being typed, as they were taken, but
    /// for those committed off its start since: Cancel types them again,
    /// and while the reading is still them, RereadKana commits its first.
    rereading: Option<Word>,
    commit: String,
    erase: Option<String>,
    send: Option<Chord>,
    effects: Vec<Effect>,
}

impl<C: Converter> Core<C> {
    pub fn new(converter: C, config: Config) -> Self {
        Self {
            converter,
            config,
            mode: Mode::Abc,
            password: false,
            sending: true,
            state: State::idle(),
            registrations: Vec::new(),
            modifiers_held: Vec::new(),
            modifier_down: None,
            held: None,
            completion: None,
            undoable: None,
            erasing: None,
            keys_waiting: Vec::new(),
            redoing: None,
            kana_run: None,
            rereading: None,
            commit: String::new(),
            erase: None,
            send: None,
            effects: Vec::new(),
        }
    }

    pub fn handle(&mut self, event: Event) -> Output {
        let before = self.mode;
        self.commit.clear();
        self.erase = None;
        self.send = None;
        self.effects.clear();
        // Only the keys that complete, and a reading picked from its list,
        // go on with a completion. A caret that may have moved leaves it, as
        // it leaves what is being typed: a host may tell of the click on the
        // list before the reading picked by it.
        if !matches!(event, Event::Key(_) | Event::Select(_) | Event::CaretMoved) {
            self.completion = None;
        }
        let consumed = match event {
            // Typed after the undo, a key goes after it, once the host is done.
            Event::Key(key) if self.erasing.is_some() => {
                self.keys_waiting.push(key);
                true
            }
            Event::Key(key) => self.key_in_field(key),
            Event::CaretMoved => {
                self.undoable = None;
                self.kana_run = None;
                false
            }
            Event::Erased(erased) => {
                self.erased(erased);
                // Taken from the application already, they can no longer
                // pass on to it, so they leave the caret where it is. One
                // undoing again starts another wait, and the keys after it
                // wait for that.
                let mut keys = mem::take(&mut self.keys_waiting).into_iter();
                for key in keys.by_ref() {
                    self.key(key);
                    if self.erasing.is_some() {
                        break;
                    }
                }
                self.keys_waiting.extend(keys);
                // Nor can they be sent as other keys.
                self.send = None;
                true
            }
            Event::FocusIn { password } => {
                self.focus_in(password);
                false
            }
            Event::FocusOut => {
                self.settle_waiting();
                self.release_modifiers();
                self.commit_visible();
                false
            }
            Event::Flush => {
                self.settle_waiting();
                self.commit_visible();
                false
            }
            Event::Select(index) => {
                self.select(index);
                false
            }
            Event::SetMode(Mode::Kana) => {
                self.enter_kana();
                false
            }
            Event::SetMode(Mode::Abc) => {
                if self.mode != Mode::Abc {
                    self.leave_kana();
                }
                false
            }
        };
        // Leaving or clicking committed what was visible; a field the focus
        // comes into holds none of it.
        if let Event::FocusIn { .. } | Event::FocusOut | Event::Flush = event {
            self.undoable = None;
            self.erasing = None;
            self.keys_waiting.clear();
            self.redoing = None;
            self.kana_run = None;
            self.rereading = None;
        }
        // A word chosen again and erased leaves what followed it.
        if !self.composing()
            && let Some(redoing) = self.redoing.take()
        {
            self.emit(&redoing.after);
        }
        // Kana read again are committed, dropped or typed again by now.
        if !self.composing() {
            self.rereading = None;
        }
        if !self.commit.is_empty() {
            self.effects.push(Effect::Typed(self.commit.clone()));
        }
        let mut output = self.output(consumed, before);
        if let Event::SetMode(_) = event {
            output.indicator = None;
        }
        output
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Tells which application has the focus, by the name
    /// [`Config::send_except`] lists it under.
    pub fn set_application(&mut self, app: &str) {
        self.sending = !self
            .config
            .send_except
            .iter()
            .any(|except| except.eq_ignore_ascii_case(app));
    }

    fn key_in_field(&mut self, key: KeyEvent) -> bool {
        let consumed = self.key(key);
        // The application may move the caret or type: what is before the
        // caret is no longer known.
        if key.kind == KeyKind::Press && !is_modifier(key.key) && (!consumed || self.send.is_some())
        {
            self.undoable = None;
            self.kana_run = None;
        }
        consumed
    }

    fn key(&mut self, event: KeyEvent) -> bool {
        if self.repeats_forget(event) {
            return true;
        }
        match event.kind {
            KeyKind::Release => {
                self.release_held(event);
                self.modifiers_held.retain(|k| *k != event.key);
                if let Some((key, at)) = self.modifier_down
                    && key == event.key
                {
                    self.modifier_down = None;
                    if event.time_ms.saturating_sub(at) <= self.config.tap_timeout_ms {
                        self.tap(key);
                    }
                }
                false
            }
            // A modifier key going down acts if its press is bound, and alone
            // it may be a tap; it goes on to the application either way.
            KeyKind::Press | KeyKind::Repeat if tappable(event.key) => {
                // A character still waiting was typed first.
                self.settle_waiting();
                // Another modifier key already down, even one whose flag is
                // the same, makes neither of them a tap.
                let first = self.modifiers_held.iter().all(|k| *k == event.key);
                let repeated = self.modifiers_held.contains(&event.key);
                // The flag of the key's own modifier is also set by its other
                // side, which only the keys held tell apart.
                let mods = self
                    .modifiers_held
                    .iter()
                    .filter(|k| **k != event.key)
                    .fold(others(event.key, event.mods), |mods, k| {
                        let flag = flag(*k);
                        Modifiers {
                            shift: mods.shift || flag.shift,
                            ctrl: mods.ctrl || flag.ctrl,
                            cmd: mods.cmd || flag.cmd,
                            alt: mods.alt || flag.alt,
                        }
                    });
                if !repeated {
                    self.modifiers_held.push(event.key);
                }
                self.modifier_down =
                    (first && alone(event.key, event.mods)).then_some((event.key, event.time_ms));
                let pressed = Chord {
                    key: event.key,
                    mods,
                };
                if !repeated && let Some(action) = self.bound(pressed, Gesture::Press) {
                    self.act(action, pressed);
                }
                false
            }
            KeyKind::Press | KeyKind::Repeat => {
                self.modifier_down = None;
                if let Some(consumed) = self.press_while_held(event) {
                    return consumed;
                }
                let pressed = Chord {
                    key: event.key,
                    mods: event.mods,
                };
                // What the key does is known only once it is let go or
                // another key is typed.
                if self.bound(pressed, Gesture::Hold).is_some() {
                    self.held = Some(Held {
                        pressed,
                        at: event.time_ms,
                        state: HeldState::Undecided(None),
                    });
                    return true;
                }
                self.press(event.key, event.mods)
            }
        }
    }

    /// Settles the question whether to forget for a key going down, before
    /// anything else sees it: `true` when it is `forget` held down, which does
    /// not answer it. Any other key withdraws it, ignored or held ones too,
    /// but `forget`, `cancel` (which withdraws it itself, staying among the
    /// candidates) and modifier keys.
    fn repeats_forget(&mut self, event: KeyEvent) -> bool {
        let State::Candidates(selection) = &self.state else {
            return false;
        };
        if !selection.forgetting
            || event.kind == KeyKind::Release
            || event.key == Key::Modifier
            || tappable(event.key)
        {
            return false;
        }
        let chord = Chord {
            key: event.key,
            mods: event.mods,
        };
        let action = self.bound(chord, Gesture::Press);
        if event.kind == KeyKind::Repeat && action == Some(Action::Forget) {
            return true;
        }
        if !matches!(action, Some(Action::Forget | Action::Cancel))
            && let State::Candidates(selection) = &mut self.state
        {
            selection.forgetting = false;
        }
        false
    }

    /// A key pressed while a key bound to be held is down; `None` when it is
    /// left to act as it does by itself.
    fn press_while_held(&mut self, event: KeyEvent) -> Option<bool> {
        let held = self.held.as_mut()?;
        // The held key repeating.
        if event.key == held.pressed.key {
            return Some(true);
        }
        let typed = Chord {
            key: event.key,
            mods: event.mods,
        };
        let character = matches!(event.key, Key::Char(_) | Key::Space)
            && !(event.mods.ctrl || event.mods.cmd || event.mods.alt);
        let long = event.time_ms.saturating_sub(held.at) > self.config.tap_timeout_ms;
        let pressed = held.pressed;
        match (held.state, character) {
            (HeldState::Undecided(None), true) if !long => {
                held.state = HeldState::Undecided(Some(typed));
                Some(true)
            }
            // Held long, or a second character while the first waits.
            (HeldState::Undecided(waiting), true) => {
                held.state = HeldState::Holding;
                if let Some(waiting) = waiting {
                    self.hold_before_syllable(pressed);
                    self.replay(waiting);
                }
                self.hold_before_syllable(pressed);
                Some(self.press(event.key, event.mods))
            }
            (HeldState::Holding, true) => {
                self.hold_before_syllable(pressed);
                Some(self.press(event.key, event.mods))
            }
            (HeldState::Undecided(waiting), false) => {
                held.state = HeldState::Alone;
                self.replay(pressed);
                if let Some(waiting) = waiting {
                    self.replay(waiting);
                }
                None
            }
            (HeldState::Holding | HeldState::Alone, false) | (HeldState::Alone, true) => None,
        }
    }

    /// A key let go: the key held, or a character typed while it was.
    fn release_held(&mut self, event: KeyEvent) {
        let Some(held) = self.held.as_mut() else {
            return;
        };
        let pressed = held.pressed;
        if event.key == pressed.key {
            let at = held.at;
            let state = held.state;
            self.held = None;
            if let HeldState::Undecided(waiting) = state {
                let quick = event.time_ms.saturating_sub(at) <= self.config.tap_timeout_ms;
                if waiting.is_some() || quick {
                    self.replay(pressed);
                }
                if let Some(waiting) = waiting {
                    self.replay(waiting);
                }
            }
            return;
        }
        // Let go before the held key: the key was held for it.
        if let HeldState::Undecided(Some(waiting)) = held.state
            && waiting.key == event.key
        {
            held.state = HeldState::Holding;
            self.hold_before_syllable(pressed);
            self.replay(waiting);
        }
    }

    /// What a held key is bound to do before a character, but begin as SKK
    /// reads a capital: held across the letters of one syllable at the start of a
    /// reading (`nyu`) it begins the reading once, and once the okurigana is
    /// marked it marks nothing more. Romaji left after kana (`kan` then `j`)
    /// is made kana first, and the okurigana marked.
    fn hold_before_syllable(&mut self, pressed: Chord) {
        let begin = self.bound(pressed, Gesture::Hold) == Some(Action::Begin);
        if !(begin && self.romaji_goes_on()) {
            self.hold(pressed);
        }
    }

    /// Whether the romaji typed so far is a syllable a letter carries on
    /// without the held key acting: the first of a reading, or one of the
    /// okurigana.
    fn romaji_goes_on(&self) -> bool {
        match &self.state {
            State::Reading(word) => {
                !word.pending.is_empty()
                    && (word.stem.as_str().is_empty() || word.okurigana.is_some())
            }
            State::Idle { .. } | State::Candidates(_) => false,
        }
    }

    /// What a held key is bound to do before a character, where it is now.
    fn hold(&mut self, pressed: Chord) {
        match self.bound(pressed, Gesture::Hold) {
            // Begin in ABC mode types the key it is bound to, and a held key
            // types nothing of its own: the character typed is all there is.
            Some(Action::Begin) if self.mode == Mode::Abc => {}
            // A mode it switches to itself does not let go of it.
            Some(action) => {
                let held = self.held.take();
                self.act(action, pressed);
                self.held = held;
            }
            None => {}
        }
    }

    /// A character waiting on the held key is typed now, after the held key
    /// pressed alone: a key other than a character came first, or the preedit
    /// is committed where it is.
    fn settle_waiting(&mut self) {
        if let Some(held) = self.held.as_mut()
            && let HeldState::Undecided(Some(waiting)) = held.state
        {
            held.state = HeldState::Alone;
            let pressed = held.pressed;
            self.replay(pressed);
            self.replay(waiting);
        }
    }

    /// A held key's release may never come, as from a platform that loses
    /// it: switching the mode lets go of the key, so nothing stays stuck
    /// past it. A character waiting on it is typed first.
    fn forget_held(&mut self) {
        self.settle_waiting();
        self.held = None;
    }

    /// A key whose press was kept from the application, acting now: the held
    /// key pressed alone, or a character typed while it was held. One to pass
    /// on goes as the character it types.
    fn replay(&mut self, typed: Chord) {
        let consumed = match self.bound(typed, Gesture::Press) {
            Some(action) => self.act(action, typed),
            // Not sent as another key: the press is gone, and a sent key
            // could not keep its place among the characters typed.
            None => self.press_plain(typed.key, typed.mods),
        };
        if consumed {
            return;
        }
        match typed.key {
            Key::Space => self.emit(" "),
            Key::Char(c) => self.emit(c.encode_utf8(&mut [0; 4])),
            _ => {}
        }
    }

    fn tap(&mut self, key: Key) {
        let tap = Chord {
            key,
            mods: Modifiers::default(),
        };
        if let Some(action) = self.bound(tap, Gesture::Tap) {
            self.act(action, tap);
        }
    }

    /// A focus change can lose the releases of keys held across it.
    fn release_modifiers(&mut self) {
        self.modifiers_held.clear();
        self.modifier_down = None;
        self.held = None;
    }

    /// Where a key is pressed: what is being typed, or with nothing typed,
    /// the input mode.
    fn scene(&self) -> Scene {
        if !self.composing() {
            return match self.mode {
                Mode::Kana => Scene::Kana,
                Mode::Abc => Scene::Abc,
            };
        }
        match &self.state {
            State::Candidates(_) => Scene::Candidates,
            State::Idle { .. } if !self.registrations.is_empty() => Scene::Registration,
            State::Reading(_) if self.listing_completion() => Scene::Completion,
            _ => Scene::Reading,
        }
    }

    /// Whether the readings of a completion are listed: one of them is shown.
    fn listing_completion(&self) -> bool {
        self.completion.as_ref().is_some_and(|c| c.index.is_some())
    }

    /// What a key, pressed so, is bound to where it is pressed. A key bound
    /// nowhere while a completion is listed does what it does in a reading.
    fn bound(&self, chord: Chord, gesture: Gesture) -> Option<Action> {
        let find = |scene| {
            self.config
                .bindings
                .get(scene)
                .iter()
                .find(|b: &&crate::Binding| b.from == chord && b.gesture == gesture)
                .map(|b| b.to)
        };
        match self.scene() {
            Scene::Completion => find(Scene::Completion).or_else(|| find(Scene::Reading)),
            scene => find(scene),
        }
    }

    /// Whether anything is being typed: a word, unfinished romaji or a
    /// registration.
    fn composing(&self) -> bool {
        !self.registrations.is_empty()
            || !matches!(&self.state, State::Idle { pending } if pending.is_empty())
    }

    /// A key does what it is bound to, or else what it does by itself.
    fn press(&mut self, key: Key, mods: Modifiers) -> bool {
        let shortcut = mods.ctrl || mods.cmd || mods.alt;
        let pressed = Chord { key, mods };
        if let Some(action) = self.bound(pressed, Gesture::Press) {
            return self.act(action, pressed);
        }
        if !self.composing() {
            if self.sending
                && let Some(remap) = self
                    .config
                    .bindings
                    .application
                    .iter()
                    .find(|b| b.from == pressed)
            {
                self.send = Some(remap.to);
                return true;
            }
        } else if shortcut {
            let pass = self.config.pass_while_composing;
            if !((mods.cmd && pass.cmd) || (mods.ctrl && pass.ctrl) || (mods.alt && pass.alt)) {
                return true;
            }
        }
        self.press_plain(key, mods)
    }

    /// A key that is not bound.
    fn press_plain(&mut self, key: Key, mods: Modifiers) -> bool {
        self.completion = None;
        if mods.ctrl || mods.cmd || mods.alt {
            self.commit_visible();
            return false;
        }
        if key == Key::Modifier {
            return false;
        }
        if key == Key::Other {
            // With only unfinished romaji, it is committed as for any key
            // passed on; a reading or the text to register stays.
            if self.registrations.is_empty()
                && let State::Idle { pending } = &mut self.state
            {
                let mut pending = mem::take(pending);
                let kana = self.flush_unfinished(&mut pending);
                self.emit(&kana);
            }
            return false;
        }
        match self.mode {
            Mode::Abc => self.direct(key),
            Mode::Kana => {
                let key = match key {
                    Key::Space if self.config.romaji.is_input_char(' ') => Key::Char(' '),
                    key => key,
                };
                match mem::replace(&mut self.state, State::idle()) {
                    State::Idle { pending } => self.idle(pending, key),
                    State::Reading(word) => self.reading(word, key),
                    State::Candidates(selection) => self.candidates(selection, key),
                }
            }
        }
    }

    /// An unbound key typed into the text to register.
    fn direct(&mut self, key: Key) -> bool {
        if self.registrations.is_empty() {
            return false;
        }
        match key {
            Key::Char(c) => self.emit(c.encode_utf8(&mut [0; 4])),
            Key::Space => self.emit(" "),
            // The preedit keeps a key the IME knows, so it does not reach the
            // application under it.
            key if named(key) => {}
            _ => return false,
        }
        true
    }

    fn idle(&mut self, mut pending: String, key: Key) -> bool {
        match key {
            Key::Char(c) if self.config.romaji.is_input_char(c) => {
                let waiting = pending.clone();
                let kana = self.config.romaji.feed(&mut pending, c);
                let run = self.registrations.is_empty().then(|| {
                    let run = self.kana_run.take();
                    self.grow_run(run, &waiting, c, &kana, &pending)
                });
                self.emit(&kana);
                // Its own kana do not end the run.
                if run.is_some() {
                    self.kana_run = run;
                }
                self.state = State::Idle { pending };
            }
            Key::Char(c) => {
                let kana = self.flush_unfinished(&mut pending);
                self.emit(&kana);
                self.emit(c.encode_utf8(&mut [0; 4]));
            }
            // Nothing to move between in the text to register, and unfinished
            // romaji stays.
            key if !self.registrations.is_empty() && named(key) && key != Key::Space => {
                self.state = State::Idle { pending };
            }
            // Passed on, a key leaves the text to register as it is.
            key if !self.registrations.is_empty() && key != Key::Space => {
                self.state = State::Idle { pending };
                return false;
            }
            _ => {
                let kana = self.flush_unfinished(&mut pending);
                self.emit(&kana);
                return self.direct(key) || self.leave_on_esc(key);
            }
        }
        true
    }

    /// The run of kana `run`, `c` typed after it made `kana` and left
    /// `pending`, from `waiting`: grown by them, or started after them by
    /// what is pending when they are not all kana. A run ended meanwhile
    /// starts with the romaji that was waiting, which put nothing in the
    /// field yet.
    fn grow_run(
        &self,
        run: Option<Word>,
        waiting: &str,
        c: char,
        kana: &str,
        pending: &str,
    ) -> Word {
        let table = &self.config.romaji;
        let typed = |keys: &str| {
            let mut word = Word::default();
            for c in keys.chars() {
                word.feed(c, table);
            }
            word
        };
        kana.chars()
            .all(is_kana)
            .then(|| {
                let mut run = run.unwrap_or_else(|| typed(waiting));
                run.feed(c, table);
                run
            })
            // The run keeps in step with what the field shows, or starts over.
            .filter(|run| run.pending == pending && run.stem.as_str().ends_with(kana))
            .unwrap_or_else(|| typed(pending))
    }

    /// Esc with nothing to cancel passes through and returns to ABC mode.
    fn leave_on_esc(&mut self, key: Key) -> bool {
        if key == Key::Esc {
            self.mode = Mode::Abc;
        }
        false
    }

    /// An unbound key in a reading: a letter is typed into it, a key the IME
    /// knows does nothing, and any other passes on.
    fn reading(&mut self, mut word: Word, key: Key) -> bool {
        match key {
            Key::Char(c) => {
                word.feed(c, &self.config.romaji);
                if word.okurigana().is_some() {
                    self.convert(word);
                } else {
                    self.state = State::Reading(word);
                }
            }
            key => {
                self.state = State::Reading(word);
                return named(key);
            }
        }
        true
    }

    /// The dictionary's candidates, then the reading as katakana, full-width
    /// and half-width, and as the letters typed for it, full-width and as
    /// typed.
    fn convert(&mut self, mut word: Word) {
        let rest = mem::take(&mut word.pending);
        let reading = word.kana();
        let candidates = self.converter.convert(&reading, word.okurigana());
        let mut selection = Selection {
            rest,
            typed: String::new(),
            word,
            converted: candidates.len(),
            candidates,
            index: 0,
            forgetting: false,
            completion: self.completion.take().map(Box::new),
        };
        self.offer_forms(&mut selection);
        self.state = State::Candidates(selection);
    }

    /// Converts `grown`, the word of `selection` with kana added to its
    /// okurigana, keeping the candidate chosen with those kana after it. A
    /// form of the reading chosen, without such a candidate, stays chosen as
    /// that form of `grown`: the dictionary may give a form too (うっ).
    fn convert_again(&mut self, selection: Selection, grown: Word) {
        let chosen = &selection.candidates[selection.index].surface;
        let form = Form::ALL.into_iter().find(|form| {
            word_form(&selection.word, *form, &self.config.romaji).as_ref() == Some(chosen)
        });
        let added = grown.kana();
        let added = added
            .strip_prefix(&selection.word.kana())
            .unwrap_or_default();
        let kept = format!("{chosen}{added}");
        self.convert(grown);
        let State::Candidates(selection) = &mut self.state else {
            return;
        };
        match selection.candidates.iter().position(|c| c.surface == kept) {
            Some(index) => selection.index = index,
            None => {
                if let Some(form) = form {
                    self.choose_form(form);
                }
            }
        }
    }

    /// Adds each form of the reading that is not a candidate yet.
    fn offer_forms(&self, selection: &mut Selection) {
        for form in &Form::ALL[1..] {
            let Some(surface) = word_form(&selection.word, *form, &self.config.romaji) else {
                continue;
            };
            if !selection.candidates.iter().any(|c| c.surface == surface) {
                selection.candidates.push(Candidate {
                    surface,
                    source: None,
                    preview: None,
                });
            }
        }
    }

    /// Selects the reading in `form`, adding it after the other candidates
    /// when it is not one of them; returns whether the reading has the form.
    fn choose_form(&mut self, form: Form) -> bool {
        let State::Candidates(selection) = &self.state else {
            return false;
        };
        match word_form(&selection.word, form, &self.config.romaji) {
            Some(surface) => {
                self.choose(surface);
                true
            }
            None => false,
        }
    }

    /// Selects `surface`, adding it after the other candidates when it is not
    /// one of them.
    fn choose(&mut self, surface: String) {
        let State::Candidates(selection) = &mut self.state else {
            return;
        };
        selection.index = match selection
            .candidates
            .iter()
            .position(|c| c.surface == surface)
        {
            Some(index) => index,
            None => {
                selection.candidates.push(Candidate {
                    surface,
                    source: None,
                    preview: None,
                });
                selection.candidates.len() - 1
            }
        };
    }

    /// An unbound key while choosing: a character going on with the romaji
    /// left after the okurigana waits after the candidate, and once it
    /// finishes kana, the okurigana takes the kana and is converted again.
    /// Any other character commits the selected candidate and is typed after
    /// it.
    fn candidates(&mut self, mut selection: Selection, key: Key) -> bool {
        selection.forgetting = false;
        match key {
            Key::Char(c) if self.goes_on(&selection, c) => {
                let mut grown = selection.word.clone();
                for c in selection.after().chars().chain([c]) {
                    grown.feed(c, &self.config.romaji);
                }
                if grown.okurigana == selection.word.okurigana {
                    selection.typed.push(c);
                    self.state = State::Candidates(selection);
                } else {
                    self.convert_again(selection, grown);
                }
                true
            }
            Key::Char(_) => {
                let index = selection.index;
                self.commit_selection(selection, index);
                let pending = match mem::replace(&mut self.state, State::idle()) {
                    State::Idle { pending } => pending,
                    _ => String::new(),
                };
                self.idle(pending, key)
            }
            key => {
                self.state = State::Candidates(selection);
                named(key)
            }
        }
    }

    /// Whether `c` goes on with the romaji still waiting after the okurigana:
    /// makes kana of it, or keeps it waiting for more. A key that drops it
    /// (the `k` of `;mo;ttk`, the `.` of `;mo;tt.`) starts something else:
    /// it types the same as it would with nothing waiting.
    fn goes_on(&self, selection: &Selection, c: char) -> bool {
        if !self.config.romaji.is_input_char(c) {
            return false;
        }
        let (_, mut waiting) = self.after_kana(selection);
        if waiting.is_empty() {
            return false;
        }
        let mut alone = String::new();
        let kana_alone = self.config.romaji.feed(&mut alone, c);
        let kana = self.config.romaji.feed(&mut waiting, c);
        (kana, waiting) != (kana_alone, alone)
    }

    /// What is typed after the okurigana: the kana it made so far, and the
    /// romaji still waiting.
    fn after_kana(&self, selection: &Selection) -> (String, String) {
        let mut pending = String::new();
        let kana = selection
            .after()
            .chars()
            .map(|c| self.config.romaji.feed(&mut pending, c))
            .collect();
        (kana, pending)
    }

    /// What a bound key does, by where it is pressed.
    fn act(&mut self, action: Action, pressed: Chord) -> bool {
        // A number the list has no reading for does nothing, the list kept.
        if let Action::Pick(n) = action
            && self.listing_completion()
        {
            self.pick_completion(usize::from(n));
            return true;
        }
        // Committing a listed reading takes it as the reading, the list gone:
        // converting it is Space's, and committing it as kana is the next
        // commit's.
        if action == Action::Commit && self.listing_completion() {
            self.completion = None;
            return true;
        }
        // Cancelling the list goes a step back, to the reading as typed:
        // cancelling that is the next cancel's.
        if action == Action::Cancel
            && self.listing_completion()
            && let Some(completion) = self.completion.take()
        {
            self.state = State::Reading(completion.typed);
            return true;
        }
        // Converting a completed reading carries the completion into the
        // candidates, to go on with from there.
        let converting = matches!(self.state, State::Reading(_))
            && matches!(action, Action::Next | Action::Previous | Action::Form(_));
        let completing = matches!(
            action,
            Action::Complete | Action::CompletePrevious | Action::NextPage | Action::PreviousPage
        );
        if !converting && !completing {
            self.completion = None;
        }
        match action {
            Action::Abc => {
                self.leave_kana();
                return true;
            }
            Action::Kana => {
                self.enter_kana();
                return true;
            }
            // A reading is typed only in kana mode; in the text to register
            // typed in ABC mode, the key types what it types.
            Action::Begin if self.mode == Mode::Abc => {
                return self.press_plain(pressed.key, pressed.mods);
            }
            _ => {}
        }
        if let Action::CommitForm(form) = action {
            let used = self.act(Action::Form(form), pressed);
            // Only the form asked for commits: a reading with no such form
            // (no hiragana for `pdf`) keeps what was selected.
            let chosen = match &self.state {
                State::Candidates(selection) => {
                    word_form(&selection.word, form, &self.config.romaji).is_some_and(|surface| {
                        selection
                            .candidates
                            .get(selection.index)
                            .is_some_and(|c| c.surface == surface)
                    })
                }
                _ => false,
            };
            if chosen
                && let State::Candidates(selection) = mem::replace(&mut self.state, State::idle())
            {
                let index = selection.index;
                self.commit_selection(selection, index);
            }
            return used;
        }
        match mem::replace(&mut self.state, State::idle()) {
            State::Idle { pending } => self.act_idle(pending, action, pressed.key),
            State::Reading(word) => self.act_reading(word, action, pressed),
            State::Candidates(selection) => self.act_candidates(selection, action),
        }
        .unwrap_or(true)
    }

    /// `None` when the key was used; `Some(false)` passes it on.
    fn act_idle(&mut self, mut pending: String, action: Action, key: Key) -> Option<bool> {
        match action {
            Action::Begin => {
                let kana = self.flush_unfinished(&mut pending);
                self.emit(&kana);
                self.state = State::Reading(Word::default());
                return None;
            }
            Action::Backspace if !pending.is_empty() => {
                pending.pop();
                self.state = State::Idle { pending };
                return None;
            }
            // Unfinished romaji is dropped.
            Action::Cancel if !pending.is_empty() => return None,
            Action::UndoCommit if pending.is_empty() && self.registrations.is_empty() => {
                self.state = State::Idle { pending };
                let Some(undoable) = self.undoable.take() else {
                    return Some(false);
                };
                // The kana typed after the commit are erased with it.
                self.kana_run = None;
                self.erase = Some(undoable.text());
                self.erasing = Some(Erasing::Commit(undoable));
                return None;
            }
            // The romaji still unfinished goes on after the kana, and waits
            // with them for the host.
            Action::RereadKana if self.registrations.is_empty() => {
                if let Some(run) = self
                    .kana_run
                    .take()
                    .filter(|run| !run.stem.is_empty() && run.pending == pending)
                {
                    self.erase = Some(run.stem.as_str().to_owned());
                    self.erasing = Some(Erasing::Kana(run));
                    return None;
                }
            }
            _ => {}
        }
        if self.registrations.is_empty() {
            let kana = self.flush_unfinished(&mut pending);
            self.emit(&kana);
            return Some(self.leave_on_esc(key));
        }
        let kana = self.flush_unfinished(&mut pending);
        self.emit(&kana);
        match action {
            Action::Commit => self.finish_registration(),
            Action::Cancel => self.cancel_registration(),
            Action::Backspace => self.erase_registration_char(),
            Action::Delete => {
                if let Some(registration) = self.registrations.last_mut() {
                    registration.text.delete();
                }
            }
            action => {
                if let (Some(registration), Some(to)) =
                    (self.registrations.last_mut(), cursor_move(action))
                {
                    registration.text.move_cursor(to);
                }
            }
        }
        None
    }

    fn act_reading(&mut self, mut word: Word, action: Action, pressed: Chord) -> Option<bool> {
        match action {
            // An empty reading is left, and a key that types a character
            // types it; any other key does nothing more.
            Action::Begin if word.is_empty() => {
                let mods = pressed.mods;
                if matches!(pressed.key, Key::Char(_)) && !(mods.ctrl || mods.cmd || mods.alt) {
                    return Some(self.press_plain(pressed.key, mods));
                }
            }
            Action::Begin => {
                word.mark_okurigana(&self.config.romaji);
                self.state = State::Reading(word);
            }
            Action::Next | Action::Previous => {
                word.flush(&self.config.romaji);
                if word.stem.is_empty() {
                    self.state = State::Reading(word);
                } else {
                    self.convert(word);
                    if action == Action::Previous
                        && let State::Candidates(selection) = &mut self.state
                    {
                        selection.index = selection.candidates.len() - 1;
                    }
                }
            }
            Action::Commit => self.commit_word(word),
            Action::Complete | Action::CompletePrevious => {
                let word = self.complete(word, action == Action::Complete);
                self.state = State::Reading(word);
            }
            Action::NextPage | Action::PreviousPage => {
                let word = self.turn_completion_page(word, action == Action::NextPage);
                self.state = State::Reading(word);
            }
            Action::Form(form) => {
                word.flush(&self.config.romaji);
                // Letters that made no kana (`pdf`) still have a form in letters.
                let letters_only = matches!(form, Form::FullAlphanumeric | Form::Alphanumeric)
                    && !word.letters(&self.config.romaji).is_empty();
                if word.stem.is_empty() && !letters_only {
                    self.state = State::Reading(word);
                } else {
                    self.convert(word);
                    self.choose_form(form);
                }
            }
            Action::Cancel if self.redoing.is_some() && self.registrations.is_empty() => {
                self.restore();
            }
            Action::Cancel if self.rereading.is_some() && self.registrations.is_empty() => {
                self.type_kana_again();
            }
            Action::Cancel => {}
            Action::RereadKana => {
                let as_taken = self.registrations.is_empty()
                    && self.rereading.as_ref().is_some_and(|taken| {
                        taken.stem.as_str() == word.stem.as_str() && taken.pending == word.pending
                    });
                if as_taken && let Some(first) = word.take_first() {
                    self.emit(&first);
                    self.rereading = Some(word.clone());
                }
                self.state = State::Reading(word);
            }
            Action::Backspace if !word.is_empty() => {
                word.backspace();
                self.state = State::Reading(word);
            }
            // Erasing past the start leaves the reading.
            Action::Backspace => {}
            // Once the okurigana is marked, the cursor stays at the end and
            // nothing is resolved, so the pending romaji still starts it.
            Action::Delete if word.okurigana.is_none() => {
                word.flush(&self.config.romaji);
                word.delete(&self.config.romaji);
                self.state = State::Reading(word);
            }
            // A word is registered for kana: letters that made none (`pdf`)
            // have no reading to register it for.
            Action::Register
                if !word.is_empty() && self.registrations.len() < MAX_REGISTRATION_DEPTH =>
            {
                let mut flushed = word.clone();
                flushed.flush(&self.config.romaji);
                if flushed.kana().is_empty() {
                    self.state = State::Reading(word);
                } else {
                    self.start_registration(flushed);
                }
            }
            action => {
                if let Some(to) = cursor_move(action)
                    && word.okurigana.is_none()
                {
                    word.flush(&self.config.romaji);
                    word.move_cursor(to, &self.config.romaji);
                }
                self.state = State::Reading(word);
            }
        }
        None
    }

    /// The word completed to the next reading, or to the previous one unless
    /// `forward`: the first completion asks the converter for the readings,
    /// and past either end is the word as typed. With nothing to complete it
    /// with, the word as it is.
    fn complete(&mut self, word: Word, forward: bool) -> Word {
        let table = &self.config.romaji;
        let completion = match self.completion.take() {
            Some(completion) => completion,
            None => {
                if word.okurigana.is_some() {
                    return word;
                }
                let mut typed = word.clone();
                typed.flush(table);
                let stem = typed.stem.as_str();
                if stem.is_empty() {
                    return word;
                }
                let mut readings: Vec<String> = Vec::new();
                for reading in self.converter.complete(stem) {
                    if reading.len() > stem.len()
                        && reading.starts_with(stem)
                        && !readings.contains(&reading)
                    {
                        readings.push(reading);
                    }
                }
                if readings.is_empty() {
                    return word;
                }
                Completion {
                    typed,
                    previews: vec![None; readings.len()],
                    readings,
                    index: None,
                }
            }
        };
        let last = completion.readings.len() - 1;
        let index = match (completion.index, forward) {
            (None, true) => Some(0),
            (None, false) => Some(last),
            (Some(i), true) => (i < last).then_some(i + 1),
            (Some(i), false) => i.checked_sub(1),
        };
        self.show_completion(completion, index)
    }

    /// The word completed to the first reading of the next page of the list
    /// shown, or of the previous page unless `forward`; past either end is
    /// the word as typed, as [`Core::complete`] goes. With no list shown,
    /// the word as it is.
    fn turn_completion_page(&mut self, word: Word, forward: bool) -> Word {
        let Some(completion) = self.completion.take() else {
            return word;
        };
        let Some(shown) = completion.index else {
            self.completion = Some(completion);
            return word;
        };
        let start = page_start(shown);
        let index = if forward {
            Some(start + PAGE_LEN).filter(|&i| i < completion.readings.len())
        } else {
            start.checked_sub(PAGE_LEN)
        };
        self.show_completion(completion, index)
    }

    /// Keeps `completion` with the reading at `index` shown, or the word as
    /// typed for `None`, and gives the word that shows it.
    fn show_completion(&mut self, completion: Completion, index: Option<usize>) -> Word {
        let table = &self.config.romaji;
        let shown = index
            .and_then(|i| completion.typed.completed(&completion.readings[i], table))
            .unwrap_or_else(|| completion.typed.clone());
        let mut completion = Completion {
            index,
            ..completion
        };
        if let Some(index) = index {
            completion.preview(&self.converter, index);
        }
        self.completion = Some(completion);
        shown
    }

    fn act_candidates(&mut self, mut selection: Selection, action: Action) -> Option<bool> {
        let len = selection.candidates.len();
        // A reading without kana (`pdf`) has nothing to register a word for.
        let can_register =
            self.registrations.len() < MAX_REGISTRATION_DEPTH && !selection.word.kana().is_empty();
        let forgetting = mem::take(&mut selection.forgetting);
        match action {
            Action::Next if selection.index + 1 < len => {
                selection.index += 1;
                self.state = State::Candidates(selection);
            }
            Action::Next | Action::Register if can_register => {
                self.start_registration(selection.into_reading());
            }
            // At the deepest registration, past the last is the first again.
            Action::Next => {
                selection.index = 0;
                self.state = State::Candidates(selection);
            }
            Action::Previous => {
                selection.index = selection.index.checked_sub(1).unwrap_or(len - 1);
                self.state = State::Candidates(selection);
            }
            // Round the pages: unlike the next candidate, past the last page
            // never starts registering.
            Action::NextPage | Action::PreviousPage => {
                let pages = len.div_ceil(PAGE_LEN);
                let page = selection.index / PAGE_LEN;
                let page = if action == Action::NextPage {
                    (page + 1) % pages
                } else {
                    (page + pages - 1) % pages
                };
                selection.index = page * PAGE_LEN;
                self.state = State::Candidates(selection);
            }
            Action::Commit => {
                let index = selection.index;
                self.commit_selection(selection, index);
            }
            // Back to the reading, completed on from the completion it was
            // converted from, or from the reading itself.
            Action::Complete | Action::CompletePrevious => {
                self.completion = selection.completion.take().map(|completion| *completion);
                let word =
                    self.complete(selection.clone().into_reading(), action == Action::Complete);
                self.state = match self.completion {
                    Some(_) => State::Reading(word),
                    None => State::Candidates(selection),
                };
            }
            Action::Form(form) => {
                self.state = State::Candidates(selection);
                self.choose_form(form);
            }
            // Backspace goes back to the reading and erases nothing: what it
            // would erase is the reading, hidden behind the candidate shown.
            // Unlike cancel, it goes back to the reading of a commit chosen
            // again too, to edit it.
            Action::Cancel | Action::Backspace if forgetting => {
                self.state = State::Candidates(selection)
            }
            Action::Cancel if self.redoing.is_some() && self.registrations.is_empty() => {
                self.restore();
            }
            // A step back: to the list of the completion converted from, as
            // it was, or else to the reading.
            Action::Cancel | Action::Backspace => {
                self.completion = selection.completion.take().map(|completion| {
                    let mut completion = *completion;
                    // The reading was converted: a candidate may have been
                    // forgotten since its candidates were asked for.
                    if let Some(index) = completion.index {
                        completion.previews[index] = None;
                        completion.preview(&self.converter, index);
                    }
                    completion
                });
                self.state = State::Reading(selection.into_reading());
            }
            Action::Forget => {
                if forgetting {
                    self.delete_selected(&mut selection);
                } else {
                    // The reading's own forms come from no dictionary.
                    selection.forgetting = selection.index < selection.converted;
                }
                self.state = State::Candidates(selection);
            }
            Action::Pick(place) => {
                self.state = State::Candidates(selection);
                self.select(usize::from(place));
            }
            // Romaji left over from the okurigana and still unfinished starts
            // the next reading.
            Action::Begin => {
                let index = selection.index;
                self.commit_selection(selection, index);
                let pending = match mem::replace(&mut self.state, State::idle()) {
                    State::Idle { pending } => pending,
                    _ => String::new(),
                };
                let mut word = Word::default();
                for c in pending.chars() {
                    word.feed(c, &self.config.romaji);
                }
                self.state = State::Reading(word);
            }
            _ => self.state = State::Candidates(selection),
        }
        None
    }

    fn delete_selected(&mut self, selection: &mut Selection) {
        // The reading's own forms come from no dictionary.
        if selection.index >= selection.converted {
            return;
        }
        let candidate = selection.candidates.remove(selection.index);
        selection.converted -= 1;
        self.effects.push(Effect::Forgotten {
            reading: selection.word.kana(),
            okurigana: selection.word.okurigana().map(str::to_owned),
            surface: candidate.surface,
        });
        // A form the dictionary also gave was not added; it is now.
        self.offer_forms(selection);
        selection.index = selection
            .index
            .min(selection.candidates.len().saturating_sub(1));
    }

    /// Commits a candidate. Romaji typed past the okurigana's first kana
    /// stays to begin the next word, however the candidate is committed.
    fn commit_selection(&mut self, selection: Selection, index: usize) {
        let keys = selection.after();
        if let Some(surface) = selection.candidates.get(index).map(|c| c.surface.clone()) {
            let committed = Undoable {
                after: self
                    .redoing
                    .as_ref()
                    .map(|r| r.after.clone())
                    .unwrap_or_default(),
                selection: Selection {
                    index,
                    rest: String::new(),
                    typed: String::new(),
                    completion: None,
                    ..selection
                },
                surface,
            };
            self.effects.push(committed.committed());
            self.emit(&committed.surface);
            if self.registrations.is_empty() {
                self.undoable = Some(committed);
            }
        }
        self.retype(&keys);
    }

    /// Once the host erased what it was asked to, goes on with it.
    fn erased(&mut self, erased: bool) {
        match self.erasing.take() {
            Some(Erasing::Commit(undoable)) => self.choose_again(undoable, erased),
            Some(Erasing::Kana(run)) => self.read_again(run, erased),
            None => {}
        }
    }

    /// Once the host erased the commit undone, chooses it again; otherwise
    /// it stays committed, and can no longer be undone.
    fn choose_again(&mut self, undoable: Undoable, erased: bool) {
        if !erased {
            return;
        }
        // Candidates are chosen in kana mode only: out of it meanwhile, the
        // commit goes back as it was, as `abc` commits what is chosen.
        if self.mode == Mode::Abc {
            self.effects.push(Effect::Erased(undoable.text()));
            self.emit(&undoable.text());
            self.undoable = Some(undoable);
            return;
        }
        self.effects.push(undoable.withdrawn());
        self.effects.push(Effect::Erased(undoable.text()));
        self.state = State::Candidates(undoable.selection.clone());
        self.redoing = Some(undoable);
    }

    /// Once the host erased the kana, reads them again; otherwise they stay
    /// in the field, and the romaji after them waits again.
    fn read_again(&mut self, run: Word, erased: bool) {
        let kana = run.stem.as_str().to_owned();
        if erased {
            self.effects.push(Effect::Erased(kana.clone()));
            // A commit before the kana can still be undone, without them.
            if let Some(undoable) = &mut self.undoable {
                match undoable.after.strip_suffix(kana.as_str()) {
                    Some(before) => undoable.after = before.to_owned(),
                    None => self.undoable = None,
                }
            }
            // A reading is typed in kana mode only: out of it meanwhile, the
            // kana go back as they were.
            if self.mode == Mode::Kana {
                self.state = State::Reading(run.clone());
                self.rereading = Some(run);
                return;
            }
            self.emit(&kana);
        }
        let mut pending = run.pending;
        match self.mode {
            Mode::Kana => self.state = State::Idle { pending },
            Mode::Abc => {
                let kana = self.flush_unfinished(&mut pending);
                self.emit(&kana);
            }
        }
    }

    /// Types the kana read again as they were taken, with the romaji that
    /// was unfinished after them, to be read again once more.
    fn type_kana_again(&mut self) {
        let Some(run) = self.rereading.take() else {
            return;
        };
        self.emit(run.stem.as_str());
        self.state = State::Idle {
            pending: run.pending.clone(),
        };
        self.kana_run = Some(run);
    }

    /// Commits again what was undone, as it was, to be undone again.
    fn restore(&mut self) {
        let Some(undoable) = self.redoing.take() else {
            return;
        };
        self.state = State::idle();
        self.effects.push(undoable.committed());
        self.emit(&undoable.text());
        self.undoable = Some(undoable);
    }

    /// Types keys a word gave back, after it, as if they were typed now: in
    /// ABC mode as they are.
    fn retype(&mut self, keys: &str) {
        if self.mode == Mode::Abc {
            self.emit(keys);
            self.state = State::idle();
            return;
        }
        let mut pending = String::new();
        let kana: String = keys
            .chars()
            .map(|c| self.config.romaji.feed(&mut pending, c))
            .collect();
        self.emit(&kana);
        self.state = State::Idle { pending };
    }

    fn select(&mut self, index: usize) {
        if self.pick_completion(index) {
            return;
        }
        match mem::replace(&mut self.state, State::idle()) {
            State::Candidates(selection)
                if index < PAGE_LEN
                    && page_start(selection.index) + index < selection.candidates.len() =>
            {
                let index = page_start(selection.index) + index;
                self.commit_selection(selection, index);
            }
            state => self.state = state,
        }
    }

    /// Completes the reading to the `n`th of the page its list shows, going
    /// on from there; whether the list shows one.
    fn pick_completion(&mut self, n: usize) -> bool {
        if !matches!(self.state, State::Reading(_)) {
            return false;
        }
        let Some(completion) = self.completion.as_mut() else {
            return false;
        };
        let Some(shown) = completion.index else {
            return false;
        };
        if n >= PAGE_LEN {
            return false;
        }
        let index = page_start(shown) + n;
        if index >= completion.readings.len() {
            return false;
        }
        let Some(word) = completion
            .typed
            .completed(&completion.readings[index], &self.config.romaji)
        else {
            return false;
        };
        completion.index = Some(index);
        completion.preview(&self.converter, index);
        self.state = State::Reading(word);
        true
    }

    fn enter_kana(&mut self) {
        if self.mode != Mode::Kana && !self.password {
            self.forget_held();
            self.mode = Mode::Kana;
        }
    }

    fn leave_kana(&mut self) {
        if self.mode != Mode::Abc {
            self.forget_held();
        }
        self.commit_inner();
        self.mode = Mode::Abc;
    }

    fn start_registration(&mut self, word: Word) {
        self.registrations.push(Registration {
            word,
            text: Editable::default(),
        });
    }

    fn finish_registration(&mut self) {
        let Some(registration) = self.registrations.pop() else {
            return;
        };
        if registration.text.is_empty() {
            self.back_to_reading(registration.word);
            return;
        }
        let mut word = registration.word;
        let rest = mem::take(&mut word.pending);
        let mut surface = registration.text.as_str().to_owned();
        if let Some(okurigana) = word.okurigana()
            && !surface.ends_with(okurigana)
        {
            surface.push_str(okurigana);
        }
        let text = self
            .converter
            .registered_text(&word.kana(), word.okurigana(), &surface);
        self.effects.push(Effect::Registered {
            reading: word.stem.as_str().to_owned(),
            okurigana: word.okurigana().map(str::to_owned),
            okurigana_head: word.okurigana_head().map(str::to_owned),
            surface,
        });
        self.effects.push(Effect::Committed {
            reading: word.kana(),
            okurigana: word.okurigana().map(str::to_owned),
            surface: text.clone(),
        });
        self.emit(&text);
        // Romaji typed after the word, as after an okurigana (持っt), goes on.
        self.retype(&rest);
    }

    fn cancel_registration(&mut self) {
        if let Some(registration) = self.registrations.pop() {
            self.back_to_reading(registration.word);
        }
    }

    /// Out of a registration, the reading it was for is typed again, in kana.
    fn back_to_reading(&mut self, word: Word) {
        self.mode = Mode::Kana;
        self.state = State::Reading(word);
    }

    fn erase_registration_char(&mut self) {
        if let Some(registration) = self.registrations.last_mut() {
            registration.text.backspace();
        }
    }

    fn focus_in(&mut self, password: bool) {
        self.mode = Mode::Abc;
        self.password = password;
        self.state = State::idle();
        self.registrations.clear();
        self.release_modifiers();
        self.effects.push(Effect::FocusMoved);
    }

    /// Commits what is visible; a registration in progress gives way to its first reading.
    fn commit_visible(&mut self) {
        let registrations = mem::take(&mut self.registrations);
        match registrations.into_iter().next() {
            Some(first) => {
                self.state = State::idle();
                self.emit(&first.word.kana());
            }
            None => self.commit_inner(),
        }
    }

    /// Resolves romaji left unfinished outside a reading at a commit.
    fn flush_unfinished(&self, pending: &mut String) -> String {
        if self.config.keep_unfinished_romaji {
            self.config.romaji.flush_keeping(pending)
        } else {
            self.config.romaji.flush(pending)
        }
    }

    fn commit_inner(&mut self) {
        match mem::replace(&mut self.state, State::idle()) {
            State::Idle { mut pending } => {
                let kana = self.flush_unfinished(&mut pending);
                self.emit(&kana)
            }
            // What is visible goes in full: romaji given back by the word is
            // resolved like any unfinished romaji.
            State::Reading(word) => {
                self.commit_word(word);
                self.commit_inner();
            }
            State::Candidates(selection) => {
                let index = selection.index;
                self.commit_selection(selection, index);
                self.commit_inner();
            }
        }
    }

    /// Commits a reading as kana. Romaji the okurigana gave back stays to
    /// begin the next word.
    fn commit_word(&mut self, mut word: Word) {
        word.flush(&self.config.romaji);
        let rest = mem::take(&mut word.pending);
        self.emit(&word.kana());
        self.retype(&rest);
    }

    /// Committed text goes into the innermost registration, if any.
    /// What followed a commit being chosen again follows the first text
    /// committed in its place.
    fn emit(&mut self, text: &str) {
        if let Some(registration) = self.registrations.last_mut() {
            registration.text.insert(text);
            return;
        }
        if text.is_empty() {
            return;
        }
        self.kana_run = None;
        let after = self.redoing.take().map(|r| r.after).unwrap_or_default();
        for text in [text, &after] {
            self.commit.push_str(text);
            if let Some(undoable) = &mut self.undoable {
                undoable.after.push_str(text);
            }
        }
    }

    fn output(&self, consumed: bool, before: Mode) -> Output {
        let marks = &self.config.marks;
        let mut preedit = String::new();
        // Romaji typed straight into the text to register waits at its cursor.
        let idle_pending = match &self.state {
            State::Idle { pending } => pending.as_str(),
            _ => "",
        };
        let last = self.registrations.len().saturating_sub(1);
        for (i, registration) in self.registrations.iter().enumerate() {
            preedit.push_str(&marks.candidate);
            self.push_word(&mut preedit, &registration.word);
            preedit.push_str(&marks.registration);
            let pending = if i == last { idle_pending } else { "" };
            push_with_cursor(
                &mut preedit,
                registration.text.split(),
                pending,
                &marks.cursor,
            );
        }
        let mut candidates = None;
        match &self.state {
            State::Idle { .. } if self.registrations.is_empty() => preedit.push_str(idle_pending),
            State::Idle { .. } => {}
            State::Reading(word) => {
                preedit.push_str(&marks.reading);
                match &word.okurigana {
                    Some(okurigana) => {
                        preedit.push_str(word.stem.as_str());
                        preedit.push_str(&marks.okurigana);
                        preedit.push_str(okurigana);
                        preedit.push_str(&word.pending);
                    }
                    None => push_with_cursor(
                        &mut preedit,
                        word.stem.split(),
                        &word.pending,
                        &marks.cursor,
                    ),
                }
                if let Some(Completion {
                    readings,
                    previews,
                    index: Some(index),
                    ..
                }) = &self.completion
                {
                    let start = page_start(*index);
                    let end = (start + PAGE_LEN).min(readings.len());
                    candidates = Some(CandidateView {
                        items: readings[start..end]
                            .iter()
                            .zip(&previews[start..end])
                            .map(|(reading, preview)| Candidate {
                                surface: reading.clone(),
                                source: None,
                                preview: preview
                                    .as_ref()
                                    .and_then(|(candidates, _)| candidates.first().cloned()),
                            })
                            .collect(),
                        selected: index - start,
                        page: start / PAGE_LEN,
                        pages: readings.len().div_ceil(PAGE_LEN),
                        more: previews[*index]
                            .as_ref()
                            .map(|(candidates, _)| candidates.iter().skip(1).cloned().collect())
                            .unwrap_or_default(),
                    });
                }
            }
            State::Candidates(selection) => {
                preedit.push_str(&marks.candidate);
                if let Some(candidate) = selection.candidates.get(selection.index) {
                    preedit.push_str(&candidate.surface);
                }
                let (kana, pending) = self.after_kana(selection);
                preedit.push_str(&kana);
                preedit.push_str(&pending);
                if selection.forgetting {
                    preedit.push_str(ASKING_TO_FORGET);
                }
                if selection.candidates.len() > 1 {
                    let start = page_start(selection.index);
                    let end = (start + PAGE_LEN).min(selection.candidates.len());
                    candidates = Some(CandidateView {
                        items: selection.candidates[start..end].to_vec(),
                        selected: selection.index - start,
                        page: start / PAGE_LEN,
                        pages: selection.candidates.len().div_ceil(PAGE_LEN),
                        more: Vec::new(),
                    });
                }
            }
        }
        if let Some(redoing) = &self.redoing {
            preedit.push_str(&redoing.after);
        }
        if let Some(Held {
            state: HeldState::Undecided(waiting),
            ..
        }) = &self.held
        {
            if marks.hold.is_empty() {
                preedit.push('\u{200B}');
            } else {
                preedit.push_str(&marks.hold);
            }
            match waiting.map(|chord| chord.key) {
                Some(Key::Char(c)) => preedit.push(c),
                Some(Key::Space) => preedit.push(' '),
                _ => {}
            }
        }
        Output {
            consumed,
            erase: self.erase.clone(),
            commit: (!self.commit.is_empty()).then(|| self.commit.clone()),
            cursor: preedit.chars().count(),
            preedit,
            candidates,
            mode: self.mode,
            indicator: (self.config.mode_indicator && self.mode != before).then_some(self.mode),
            send: self.send,
            effects: self.effects.clone(),
        }
    }

    fn push_word(&self, preedit: &mut String, word: &Word) {
        preedit.push_str(word.stem.as_str());
        if let Some(okurigana) = &word.okurigana {
            preedit.push_str(&self.config.marks.okurigana);
            preedit.push_str(okurigana);
        }
    }
}

/// Text edited at a cursor, with the cursor mark where it is unless it is at
/// the end. Applications draw their own cursor inside the preedit unreliably,
/// so theirs always stays at the end.
fn push_with_cursor(
    preedit: &mut String,
    (before, after): (&str, &str),
    pending: &str,
    mark: &str,
) {
    preedit.push_str(before);
    preedit.push_str(pending);
    if !after.is_empty() {
        preedit.push_str(mark);
        preedit.push_str(after);
    }
}

/// A modifier key whose tap, pressed alone and let go, can be bound.
fn tappable(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::CtrlLeft
            | Key::CtrlRight
            | Key::CmdLeft
            | Key::CmdRight
            | Key::AltLeft
            | Key::AltRight
    )
}

/// A key that changes nothing in the field by itself.
fn is_modifier(key: Key) -> bool {
    tappable(key) || key == Key::Modifier
}

/// Whether no modifier but the key's own is held with it.
fn alone(key: Key, mods: Modifiers) -> bool {
    others(key, mods) == Modifiers::default()
}

/// The modifiers held with a modifier key, but its own.
fn others(key: Key, mods: Modifiers) -> Modifiers {
    let own = flag(key);
    Modifiers {
        shift: mods.shift && !own.shift,
        ctrl: mods.ctrl && !own.ctrl,
        cmd: mods.cmd && !own.cmd,
        alt: mods.alt && !own.alt,
    }
}

/// The modifier a modifier key sets; none for another key.
fn flag(key: Key) -> Modifiers {
    Modifiers {
        shift: matches!(key, Key::ShiftLeft | Key::ShiftRight),
        ctrl: matches!(key, Key::CtrlLeft | Key::CtrlRight),
        cmd: matches!(key, Key::CmdLeft | Key::CmdRight),
        alt: matches!(key, Key::AltLeft | Key::AltRight),
    }
}

fn cursor_move(action: Action) -> Option<CursorMove> {
    match action {
        Action::Left => Some(CursorMove::Left),
        Action::Right => Some(CursorMove::Right),
        Action::Home => Some(CursorMove::Home),
        Action::End => Some(CursorMove::End),
        _ => None,
    }
}

/// A key the IME knows by name. While something is being typed, it never
/// reaches the application unbound, so the application cannot act on the
/// text under the preedit.
fn named(key: Key) -> bool {
    matches!(
        key,
        Key::Space
            | Key::Enter
            | Key::Esc
            | Key::Backspace
            | Key::Delete
            | Key::Left
            | Key::Right
            | Key::Up
            | Key::Down
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::F(6..=10)
            | Key::Eisu
            | Key::Kana
            | Key::Henkan
            | Key::Muhenkan
    )
}

/// The reading in `form`, or `None` when it has no such form (no hiragana
/// for letters that made no kana).
fn word_form(word: &Word, form: Form, table: &romaji::RomajiTable) -> Option<String> {
    let kana = word.kana();
    let surface = match form {
        Form::Hiragana => kana,
        Form::Katakana => romaji::katakana(&kana),
        Form::HalfKatakana => romaji::half_katakana(&kana),
        Form::FullAlphanumeric => romaji::full_width(&word.letters(table)),
        Form::Alphanumeric => word.letters(table),
    };
    Some(surface).filter(|s| !s.is_empty())
}

/// A kana a reading can be made of, as typed from romaji.
fn is_kana(c: char) -> bool {
    matches!(c, 'ぁ'..='ゖ' | 'ゝ' | 'ゞ' | 'ー')
}

/// Where the page holding the candidate at `index` starts.
fn page_start(index: usize) -> usize {
    index / PAGE_LEN * PAGE_LEN
}
