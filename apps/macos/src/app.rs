//! The Input Method Kit server. Its controller turns `NSEvent`s into core
//! events and draws the core's output; it keeps no input state of its own
//! beyond what talking to the client needs.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::ptr::NonNull;
use std::time::Duration;

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchTime};
use kanaemi_core::{Chord, Event, Key, KeyEvent, KeyKind, Modifiers, Output};
use kanaemi_runtime::{Field, Profile};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, Bool, NSObject};
use objc2::{
    AnyThread, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class,
    msg_send, sel,
};
use objc2_app_kit::{
    NSApplication, NSEvent, NSEventMask, NSEventType, NSMenu, NSMenuItem,
    NSUnderlineStyleAttributeName,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
};
use objc2_foundation::{
    NSAttributedString, NSBundle, NSDictionary, NSNotFound, NSNumber, NSProcessInfo, NSRange,
    NSString, NSUInteger,
};
use objc2_input_method_kit::{IMKInputController, IMKServer};

use crate::keys::{Keys, POSTED_MARK, RawEvent, RawKind, key_to_send, utf16_offset};
use crate::posted::{self, Posted, Purpose};
use crate::{
    accessibility, candidate_window, handled, indicator, input_monitoring, key_tap, meaning,
    secure_input,
};

const CONNECTION_NAME: &str = "io.github.kanaemi-app.inputmethod.Kanaemi_Connection";

thread_local! {
    /// The settings and the engine every client shares.
    static PROFILE: RefCell<Option<Profile>> = const { RefCell::new(None) };
    /// The Shift keys are one keyboard's, whichever client has the focus.
    static KEYS: RefCell<Keys> = RefCell::new(Keys::default());
    /// The controller of the field with the focus, for other programs to set
    /// the mode of.
    static ACTIVE: RefCell<Option<Retained<KanaemiController>>> = const { RefCell::new(None) };
    /// Whether the keys down are to be looked at again soon.
    static WATCHING: Cell<bool> = const { Cell::new(false) };
    /// The keys posted to the application, whichever client has the focus.
    static POSTED: RefCell<Posted> = RefCell::new(Posted::default());
    /// Told of every click in another application, kept for as long as
    /// the IME runs.
    static CLICKS: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    /// Keys handled without changing the text, marked through until their
    /// application is done with them, oldest first.
    static UNSEEN: RefCell<VecDeque<Unseen>> = const { RefCell::new(VecDeque::new()) };
}

/// A key handled without changing the text, marked through with text that
/// shows nothing: the text to erase and the key to send wait until the mark
/// is taken back, as an application takes a key that comes while text is
/// marked for the input method's.
struct Unseen {
    controller: Retained<KanaemiController>,
    client: Retained<AnyObject>,
    erase: Option<String>,
    send: Option<Chord>,
}

/// Whether text is being erased: Backspaces posted and not yet back, or
/// waiting for a mark to go before they are posted.
fn erasing() -> bool {
    POSTED.with_borrow(Posted::erasing)
        || UNSEEN.with_borrow(|unseen| unseen.iter().any(|u| u.erase.is_some()))
}

/// Takes back the mark through the oldest key handled without changing the
/// text, then erases and sends what the key asked for. Run from the main
/// queue, once the application is done with the key.
fn unmark_unseen() {
    let Some(unseen) = UNSEEN.with_borrow_mut(VecDeque::pop_front) else {
        return;
    };
    // A key since may have marked text of its own in place of the mark.
    if !unseen.controller.ivars().marked.get() {
        set_marked_text(&unseen.client, "", 0);
        // A call that answers returns once the client has taken the mark
        // back, so the keys posted next reach it with nothing marked.
        let _: NSRange = unsafe { msg_send![&*unseen.client, markedRange] };
    }
    if let Some(text) = unseen.erase {
        unseen.controller.erase(&text, Some(&unseen.client));
    }
    if let Some(chord) = unseen.send {
        send_key(chord, Purpose::InPlace);
    }
}

fn with_profile<T>(f: impl FnOnce(&mut Profile) -> T) -> T {
    PROFILE.with_borrow_mut(|profile| {
        f(profile
            .as_mut()
            .expect("the profile is opened before any controller"))
    })
}

/// How often the keys down are looked at for one let go: well within the
/// time that tells a key held from one pressed alone.
const LIFT_POLL: Duration = Duration::from_millis(10);

/// Whether the keyboard has the key down now. Input Method Kit passes on
/// presses but not releases, which a key bound to be held needs.
fn key_down(code: u16) -> bool {
    CGEventSource::key_state(CGEventSourceStateID::HIDSystemState, code)
}

/// Now, on the clock of `NSEvent`'s timestamps.
fn now_ms() -> u64 {
    (NSProcessInfo::processInfo().systemUptime() * 1000.0) as u64
}

/// Whether a key is bound to be held, the only use of the keys let go.
fn hold_bound() -> bool {
    with_profile(|profile| profile.config().bindings.hold_a_key())
}

/// The keys let go: as the tap saw them, with those it lost; or without
/// it, those the keyboard says are up now, as of `time_ms`.
fn lifted_keys(time_ms: u64) -> Vec<KeyEvent> {
    KEYS.with_borrow_mut(|keys| {
        if key_tap::running() {
            let now = now_ms();
            let mut lifted = keys.tapped_releases(now);
            lifted.extend(keys.lost(key_down, now));
            lifted
        } else {
            keys.lifted(key_down, time_ms)
        }
    })
}

/// Looks at the keys again soon, and on until each is let go: to find them
/// up, and with the tap, to drop what never reaches the IME.
fn watch_keys() {
    let pending =
        KEYS.with_borrow(|keys| keys.watching() || (key_tap::running() && keys.waiting()));
    if WATCHING.get() || !pending || !hold_bound() {
        return;
    }
    let Ok(when) = DispatchTime::try_from(LIFT_POLL) else {
        return;
    };
    WATCHING.set(true);
    let _ = DispatchQueue::main().after(when, look_at_keys);
}

fn look_at_keys() {
    WATCHING.set(false);
    guarded((), release_keys);
    watch_keys();
}

/// Feeds `event` to the field with the focus.
fn tell_active(event: Event) {
    let Some(active) = ACTIVE.with_borrow(|active| active.clone()) else {
        return;
    };
    let client: Option<Retained<AnyObject>> = unsafe { msg_send![&*active, client] };
    active.dispatch(event, client.as_deref());
}

/// Tells the active field the text is erased, from the main queue: the last
/// Backspace passed on is handled by the application before anything queued
/// after it runs.
fn erased_later() {
    DispatchQueue::main().exec_async(|| guarded((), || tell_active(Event::Erased(true))));
}

/// Once the Backspaces erasing text are waited for too long, tells the
/// active field they never came, so the keys typed meanwhile go on.
fn give_up_erasing_later() {
    let Ok(when) = DispatchTime::try_from(Duration::from_millis(posted::WAIT_MS + 1)) else {
        return;
    };
    let _ = DispatchQueue::main().after(when, || {
        guarded((), || {
            if POSTED.with_borrow_mut(|posted| posted.expire(now_ms())) {
                tracing::warn!("the keys erasing text never came back");
                tell_active(Event::Erased(false));
            }
        });
    });
}

/// Watches every click in other applications: one may move the caret of
/// the field with the focus, where the core can no longer tell what is
/// before it. Watching mouse buttons asks for no permission.
fn watch_clicks() {
    let mask =
        NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;
    let handler = RcBlock::new(|_event: NonNull<NSEvent>| {
        guarded((), || tell_active(Event::CaretMoved));
    });
    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &handler);
    if monitor.is_none() {
        tracing::warn!("clicks not watched");
    }
    CLICKS.set(monitor);
}

/// What the tap saw, kept; it is passed on from the main queue, not from
/// the tap, which macOS turns off when it takes long.
fn tapped(code: u16, down: bool, time_ms: u64) {
    guarded((), || {
        KEYS.with_borrow_mut(|keys| keys.tapped(code, down, time_ms));
        DispatchQueue::main().exec_async(|| {
            guarded((), || {
                if hold_bound() {
                    release_keys();
                    watch_keys();
                }
            });
        });
    });
}

/// Turns the tap on or off as a key is bound to be held or not, starting it
/// once the permission is there: the next field after it is granted picks
/// it up.
fn follow_hold() {
    let hold = hold_bound();
    key_tap::want(hold);
    if hold && !key_tap::running() && key_tap::start(tapped) {
        tracing::info!("watching keys let go with an event tap");
    }
}

/// Feeds the field with the focus the keys let go.
fn release_keys() {
    match ACTIVE.with_borrow(Clone::clone) {
        Some(controller) => {
            let client: Option<Retained<AnyObject>> = unsafe { msg_send![&*controller, client] };
            controller.release_lifted(client.as_deref(), now_ms());
        }
        // No field has the focus to tell: the keys are only forgotten.
        None => {
            lifted_keys(now_ms());
        }
    }
}

fn new_field() -> Field {
    with_profile(|profile| Field::new(profile))
}

pub struct Ivars {
    field: RefCell<Field>,
    marked: Cell<bool>,
}

define_class!(
    // SAFETY: IMKInputController is designed to be subclassed; no Drop is added.
    #[unsafe(super(IMKInputController, NSObject))]
    #[name = "KanaemiController"]
    #[ivars = Ivars]
    struct KanaemiController;

    impl KanaemiController {
        #[unsafe(method_id(initWithServer:delegate:client:))]
        fn init_with_server(
            this: Allocated<Self>,
            server: Option<&IMKServer>,
            delegate: Option<&AnyObject>,
            client: Option<&AnyObject>,
        ) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars {
                field: RefCell::new(new_field()),
                marked: Cell::new(false),
            });
            unsafe { msg_send![super(this), initWithServer: server, delegate: delegate, client: client] }
        }

        /// The input source menu in the menu bar; its items' actions are
        /// sent to this controller.
        #[unsafe(method_id(menu))]
        fn menu(&self) -> Retained<NSMenu> {
            let mtm = MainThreadMarker::new().expect("Input Method Kit calls on the main thread");
            let menu = NSMenu::new(mtm);
            if let Some(holder) = secure_input::check() {
                let warning = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(&holder.label()),
                        None,
                        &NSString::from_str(""),
                    )
                };
                warning.setEnabled(false);
                menu.addItem(&warning);
                menu.addItem(&NSMenuItem::separatorItem(mtm));
            }
            if with_profile(|profile| input_monitoring::missing(profile.config())) {
                let warning = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(input_monitoring::WARNING),
                        None,
                        &NSString::from_str(""),
                    )
                };
                warning.setEnabled(false);
                menu.addItem(&warning);
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str("入力監視の設定を開く…"),
                        Some(sel!(openInputMonitoring:)),
                        &NSString::from_str(""),
                    )
                };
                menu.addItem(&item);
                menu.addItem(&NSMenuItem::separatorItem(mtm));
            }
            if with_profile(|profile| accessibility::missing(profile.config())) {
                let warning = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(accessibility::WARNING),
                        None,
                        &NSString::from_str(""),
                    )
                };
                warning.setEnabled(false);
                menu.addItem(&warning);
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str("アクセシビリティの設定を開く…"),
                        Some(sel!(openAccessibility:)),
                        &NSString::from_str(""),
                    )
                };
                menu.addItem(&item);
                menu.addItem(&NSMenuItem::separatorItem(mtm));
            }
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str("設定を開く…"),
                    Some(sel!(openSettings:)),
                    &NSString::from_str(""),
                )
            };
            menu.addItem(&item);
            menu
        }

        #[unsafe(method(openSettings:))]
        fn open_settings(&self, _sender: Option<&AnyObject>) {
            guarded((), open_settings_app);
        }

        #[unsafe(method(openInputMonitoring:))]
        fn open_input_monitoring(&self, _sender: Option<&AnyObject>) {
            guarded((), open_input_monitoring);
        }

        #[unsafe(method(openAccessibility:))]
        fn open_accessibility(&self, _sender: Option<&AnyObject>) {
            guarded((), open_accessibility);
        }

        #[unsafe(method(recognizedEvents:))]
        fn recognized_events(&self, _sender: Option<&AnyObject>) -> NSUInteger {
            // Asking for flagsChanged turns off IMK's own commit on a click
            // outside the preedit, so mouse-down is asked for too. Key-up
            // tells a key held from one pressed alone.
            (NSEventMask::KeyDown
                | NSEventMask::KeyUp
                | NSEventMask::FlagsChanged
                | NSEventMask::LeftMouseDown)
                .0 as NSUInteger
        }

        #[unsafe(method(handleEvent:client:))]
        fn handle_event(&self, event: Option<&NSEvent>, sender: Option<&AnyObject>) -> Bool {
            guarded(Bool::NO, || self.handle_event_inner(event, sender))
        }

        #[unsafe(method(activateServer:))]
        fn activate_server(&self, sender: Option<&AnyObject>) {
            guarded((), || self.activate(sender));
        }

        #[unsafe(method(deactivateServer:))]
        fn deactivate_server(&self, sender: Option<&AnyObject>) {
            guarded((), || self.deactivate(sender));
        }

        #[unsafe(method(commitComposition:))]
        fn commit_composition(&self, sender: Option<&AnyObject>) {
            guarded((), || {
                self.dispatch(Event::Flush, sender);
            });
        }

        #[unsafe(method(hidePalettes))]
        fn hide_palettes(&self) {
            guarded((), || {
                // The candidate window is Kanaemi's own, unknown to Input
                // Method Kit, which hides only the palettes it keeps.
                candidate_window::hide();
                let _: () = unsafe { msg_send![super(self), hidePalettes] };
            });
        }
    }
);

impl KanaemiController {
    fn handle_event_inner(&self, event: Option<&NSEvent>, sender: Option<&AnyObject>) -> Bool {
        let Some(event) = event else { return Bool::NO };
        let kind = match event.r#type() {
            NSEventType::KeyDown => RawKind::KeyDown,
            NSEventType::KeyUp => RawKind::KeyUp,
            NSEventType::FlagsChanged => RawKind::FlagsChanged,
            NSEventType::LeftMouseDown => {
                self.dispatch(Event::Flush, sender);
                return Bool::NO;
            }
            _ => RawKind::Other,
        };
        let typed = matches!(kind, RawKind::KeyDown | RawKind::KeyUp);
        let characters = typed
            .then(|| event.characters())
            .flatten()
            .map(|s| s.to_string());
        let ignoring = typed
            .then(|| event.charactersIgnoringModifiers())
            .flatten()
            .map(|s| s.to_string());
        let raw = RawEvent {
            kind,
            key_code: event.keyCode(),
            flags: event.modifierFlags().0,
            characters: characters.as_deref(),
            characters_ignoring_modifiers: ignoring.as_deref(),
            time_ms: (event.timestamp() * 1000.0) as u64,
            user_data: event.CGEvent().map_or(0, |event| {
                CGEvent::integer_value_field(Some(&event), CGEventField::EventSourceUserData)
            }),
            repeat: typed && event.isARepeat(),
        };
        tracing::debug!(?raw, "key");
        // A key the IME posted goes on to the application unseen by the core:
        // it is what the core asked for. The release of a key never reaches
        // the IME, so the press is what tells the application has it.
        if raw.kind == RawKind::KeyDown
            && let Some(purpose) = POSTED.with_borrow_mut(|p| p.take(raw.key_code, raw.time_ms))
        {
            if purpose == Purpose::Erasing && !POSTED.with_borrow(Posted::erasing) {
                erased_later();
            }
            return Bool::NO;
        }
        // A field focused before Kanaemi was started or replaced is never
        // activated, yet its keys come here; the keys let go are its too.
        if !ACTIVE.with_borrow(|active| {
            active
                .as_deref()
                .is_some_and(|active| std::ptr::eq(active, self))
        }) {
            self.make_active();
            self.note_application(sender);
        }
        // A key let go before this one goes first, as it did on the keyboard,
        // so it was let go no later than this one was pressed: the main thread
        // may get to this event well after it happened.
        let hold = hold_bound();
        if hold {
            KEYS.with_borrow_mut(|keys| keys.pressing(raw));
            self.release_lifted(sender, raw.time_ms.min(now_ms()));
        }
        if let Some(again) = KEYS.with_borrow_mut(|keys| keys.pressed_again(raw))
            && hold
        {
            self.dispatch(Event::Key(again), sender);
        }
        let consumed = match KEYS.with_borrow_mut(|keys| keys.translate(raw)) {
            Some(key) => self.dispatch(Event::Key(key), sender),
            None => false,
        };
        // The tap may have seen keys let go after this one was pressed.
        if hold && key_tap::running() {
            self.release_lifted(sender, now_ms());
        }
        watch_keys();
        Bool::new(consumed)
    }

    /// Posts a Backspace for each grapheme of `text`; the core hears whether
    /// it is gone once the application has handled them, or that it is not
    /// once they are waited for too long. Returns whether they were sent; the
    /// key that asked for them goes on as it is when not.
    fn erase(&self, text: &str, client: Option<&AnyObject>) -> bool {
        let presses = kanaemi_runtime::backspaces(text);
        let backspace = Chord {
            key: Key::Backspace,
            mods: Modifiers::default(),
        };
        if (0..presses).all(|_| send_key(backspace, Purpose::Erasing)) {
            give_up_erasing_later();
            return true;
        }
        self.dispatch(Event::Erased(false), client);
        false
    }

    /// Feeds the core the release of each key let go since the keys were
    /// last looked at.
    fn release_lifted(&self, client: Option<&AnyObject>, time_ms: u64) {
        for event in lifted_keys(time_ms) {
            self.dispatch(Event::Key(event), client);
        }
    }

    fn make_active(&self) {
        ACTIVE.set(Some(self.retain()));
        input_monitoring::note();
        accessibility::note();
        follow_hold();
    }

    /// Tells the field which application `client` is in.
    fn note_application(&self, client: Option<&AnyObject>) {
        if let Some(app) = client.and_then(bundle_identifier) {
            self.ivars().field.borrow_mut().set_application(app);
        }
    }

    fn activate(&self, sender: Option<&AnyObject>) {
        self.make_active();
        secure_input::check();
        self.note_application(sender);
        // macOS turns input methods off in a secure field, so a field the
        // IME sees is never a password field.
        self.dispatch(Event::FocusIn { password: false }, sender);
        // The settings are read again as the focus comes in.
        follow_hold();
    }

    fn deactivate(&self, sender: Option<&AnyObject>) {
        self.dispatch(Event::FocusOut, sender);
        candidate_window::hide();
        // Another client may have been activated first.
        ACTIVE.with_borrow_mut(|active| {
            if active
                .as_deref()
                .is_some_and(|active| std::ptr::eq(active, self))
            {
                *active = None;
                key_tap::want(false);
            }
        });
    }

    /// Feeds one event to the core and draws the result; returns whether the
    /// key was consumed. A panic clears the preedit, starts the field over and
    /// hands the key to the application.
    fn dispatch(&self, event: Event, client: Option<&AnyObject>) -> bool {
        let ivars = self.ivars();
        let handled = catch_unwind(AssertUnwindSafe(|| {
            let marked_before = ivars.marked.get();
            let output = with_profile(|profile| ivars.field.borrow_mut().handle(profile, event));
            tracing::debug!(?event, ?output, "handled");
            let pressed = matches!(
                event,
                Event::Key(KeyEvent {
                    kind: KeyKind::Press | KeyKind::Repeat,
                    ..
                })
            );
            // Many applications, terminals among them, pass on a key the
            // input method handled unless it changed the text: it is marked
            // through, and what it erases or sends waits for the mark to go.
            // Without the permission to post keys, nothing is erased or sent,
            // and the key goes on as below. Marked text with nothing marked
            // takes the place of the selection, so a selection is left alone.
            // While text is being erased, the core keeps the keys typed and
            // changes nothing for them; a mark then would take the
            // Backspaces on their way in place of the text.
            let posts = output.erase.is_some() || output.send.is_some();
            if pressed
                && let Some(client) = client
                && handled::changes_nothing(&output, marked_before)
                && (!posts || accessibility::trusted())
                && !erasing()
                && selection_is_empty(client)
            {
                self.draw(&output, client);
                set_marked_text(client, handled::UNSEEN, 0);
                UNSEEN.with_borrow_mut(|unseen| {
                    unseen.push_back(Unseen {
                        controller: self.retain(),
                        client: client.retain(),
                        erase: output.erase.clone(),
                        send: output.send,
                    });
                });
                DispatchQueue::main().exec_async(|| guarded((), unmark_unseen));
                return true;
            }
            let erasing = output
                .erase
                .as_deref()
                .is_none_or(|text| self.erase(text, client));
            if let Some(client) = client {
                self.draw(&output, client);
            }
            match output.send {
                // Without the permission to send keys, the pressed key goes on as
                // it is, which most applications still handle.
                Some(chord) => send_key(chord, Purpose::InPlace),
                None => output.consumed && erasing,
            }
        }));
        match handled {
            Ok(consumed) => consumed,
            Err(_) => {
                // The event is left out: it may be a key the user typed.
                tracing::warn!("handling an event panicked; the state was reset");
                with_profile(|profile| ivars.field.borrow_mut().restart(profile));
                if let Some(client) = client {
                    set_marked_text(client, "", 0);
                }
                ivars.marked.set(false);
                candidate_window::hide();
                false
            }
        }
    }

    fn draw(&self, output: &Output, client: &AnyObject) {
        let ivars = self.ivars();
        if let Some(mode) = output.indicator {
            indicator::show(mode, client);
        }
        if let Some(text) = &output.commit {
            let text = NSString::from_str(text);
            let nowhere = NSRange::new(NSNotFound as usize, 0);
            let _: () = unsafe { msg_send![client, insertText: &*text, replacementRange: nowhere] };
            ivars.marked.set(false);
        }
        if !output.preedit.is_empty() || ivars.marked.get() {
            set_marked_text(
                client,
                &output.preedit,
                utf16_offset(&output.preedit, output.cursor),
            );
            ivars.marked.set(!output.preedit.is_empty());
        }
        match &output.candidates {
            Some(view) => candidate_window::show(view, client),
            None => candidate_window::hide(),
        }
    }
}

/// The preedit carries its state as marks in the text, so it is drawn without
/// the underline clients add to marked text by default.
fn set_marked_text(client: &AnyObject, text: &str, cursor_utf16: usize) {
    let none: Retained<AnyObject> = Retained::into_super(Retained::into_super(
        Retained::into_super(NSNumber::new_isize(0)),
    ));
    let plain: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_retained_objects(&[unsafe { NSUnderlineStyleAttributeName }], &[none]);
    let text = unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(text),
            Some(&*plain),
        )
    };
    let selection = NSRange::new(cursor_utf16, 0);
    let nowhere = NSRange::new(NSNotFound as usize, 0);
    let _: () = unsafe {
        msg_send![client, setMarkedText: &*text, selectionRange: selection, replacementRange: nowhere]
    };
}

/// Whether `client` has no text selected, only a caret.
fn selection_is_empty(client: &AnyObject) -> bool {
    let selection: NSRange = unsafe { msg_send![client, selectedRange] };
    selection.length == 0
}

/// The bundle identifier of the application `client` is in.
fn bundle_identifier(client: &AnyObject) -> Option<String> {
    let id: Option<Retained<NSString>> = unsafe { msg_send![client, bundleIdentifier] };
    id.map(|id| id.to_string())
}

/// Posts `chord` as a key press and release for `purpose`, kept until it
/// comes back to the IME; returns whether it was sent. Posting needs the
/// Accessibility permission.
fn send_key(chord: Chord, purpose: Purpose) -> bool {
    let Some((code, flags)) = key_to_send(chord) else {
        tracing::warn!(?chord, "no key code to send");
        return false;
    };
    if !accessibility::trusted() {
        // Once is enough: without the permission every remapped key ends here.
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| {
            tracing::warn!("not allowed to send keys; grant Kanaemi the Accessibility permission");
        });
        return false;
    }
    for down in [true, false] {
        let Some(event) = CGEvent::new_keyboard_event(None, code, down) else {
            return false;
        };
        CGEvent::set_flags(Some(&event), CGEventFlags(flags));
        CGEvent::set_integer_value_field(
            Some(&event),
            CGEventField::EventSourceUserData,
            POSTED_MARK,
        );
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }
    POSTED.with_borrow_mut(|posted| posted.post(code, now_ms(), purpose));
    true
}

/// Runs an Input Method Kit callback: a panic must not unwind into AppKit,
/// which would end the IME.
fn guarded<T>(fallback: T, callback: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(callback)).unwrap_or_else(|_| {
        tracing::warn!("an input method callback panicked");
        fallback
    })
}

/// The settings app ships inside the IME's bundle; `open` brings an open
/// one to the front instead of starting a second.
fn open_settings_app() {
    let Some(dir) = NSBundle::mainBundle().resourcePath() else {
        tracing::warn!("no resource folder to find the settings app in");
        return;
    };
    let app = PathBuf::from(dir.to_string()).join(SETTINGS_APP);
    if !app.exists() {
        tracing::warn!(app = %app.display(), "settings app missing from the bundle");
        return;
    }
    open(&[app.as_os_str()]);
}

/// Opens the Input Monitoring settings, with Kanaemi.app selected in Finder
/// beside them to drag in: an input method is not listed there until it is
/// added by hand.
fn open_input_monitoring() {
    open(&[OsStr::new(input_monitoring::SETTINGS_URL)]);
    reveal_bundle();
}

/// Opens the Accessibility settings, with Kanaemi.app beside them as for
/// Input Monitoring.
fn open_accessibility() {
    open(&[OsStr::new(accessibility::SETTINGS_URL)]);
    reveal_bundle();
}

/// Selects Kanaemi.app in Finder, to drag into a permission's list.
fn reveal_bundle() {
    let bundle = NSBundle::mainBundle().bundlePath().to_string();
    open(&[OsStr::new("-R"), OsStr::new(&bundle)]);
}

/// Runs `open` with `args`, without waiting on the main thread.
fn open(args: &[&OsStr]) {
    match std::process::Command::new("/usr/bin/open")
        .args(args)
        .spawn()
    {
        // Waited for on a thread of its own, so no finished `open` lingers.
        Ok(mut open) => {
            let waiting = std::thread::Builder::new()
                .name("launcher".to_owned())
                .spawn(move || open.wait());
            if let Err(error) = waiting {
                tracing::warn!(%error, "launcher not waited for");
            }
        }
        Err(error) => tracing::warn!(?args, %error, "not opened"),
    }
}

const SETTINGS_APP: &str = "KanaemiSettings.app";

/// Answers the requests other programs sent, on the main thread, where the
/// fields are; a mode to set goes to the field with the focus first.
fn serve_control() {
    guarded((), || {
        for request in with_profile(Profile::take_control_requests) {
            if let Some(mode) = request.mode_to_set()
                && let Some(controller) = ACTIVE.with_borrow(Clone::clone)
            {
                let client: Option<Retained<AnyObject>> =
                    unsafe { msg_send![&*controller, client] };
                controller.dispatch(Event::SetMode(mode), client.as_deref());
            }
            with_profile(|profile| profile.answer(request));
        }
    });
}

pub fn run() {
    kanaemi_runtime::init_logging();
    let mtm = MainThreadMarker::new().expect("Input Method Kit servers run on the main thread");
    let dir = kanaemi_config::dir().unwrap_or_else(|| {
        tracing::warn!("HOME is not set; no settings or dictionary is kept");
        std::env::temp_dir().join("kanaemi")
    });
    PROFILE.set(Some(Profile::open(dir)));
    with_profile(|profile| profile.listen(|| DispatchQueue::main().exec_async(serve_control)));
    // IMKServer looks the class up by its Info.plist name, so register it first.
    let _ = KanaemiController::class();
    watch_clicks();

    let bundle_id = NSBundle::mainBundle().bundleIdentifier();
    let name = NSString::from_str(CONNECTION_NAME);
    let server = unsafe {
        IMKServer::initWithName_bundleIdentifier(
            IMKServer::alloc(),
            Some(&name),
            bundle_id.as_deref(),
        )
    }
    .expect("IMKServer could not be created");

    candidate_window::on_pick(|index| guarded((), || tell_active(Event::Select(index))));
    meaning::start(|surface, meaning| {
        guarded((), || candidate_window::meaning_found(surface, meaning));
    });

    tracing::info!(version = kanaemi_core::VERSION, "kanaemi started");
    NSApplication::sharedApplication(mtm).run();
    drop(server);
}
