//! The Input Method Kit server. Its controller turns `NSEvent`s into core
//! events and draws the core's output; it keeps no input state of its own
//! beyond what talking to the client needs.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use dispatch2::DispatchQueue;
use kanaemi_core::{Chord, Event, Output};
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
use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags, CGEventTapLocation};
use objc2_foundation::{
    NSArray, NSAttributedString, NSBundle, NSDictionary, NSNotFound, NSNumber, NSRange, NSString,
    NSUInteger,
};
use objc2_input_method_kit::{
    IMKCandidates, IMKCandidatesSendServerKeyEventFirst, IMKInputController, IMKServer,
    kIMKLocateCandidatesBelowHint, kIMKSingleColumnScrollingCandidatePanel,
};

use crate::keys::{Keys, POSTED_MARK, RawEvent, RawKind, key_to_send, utf16_offset};
use crate::{candidates, indicator, secure_input};

const CONNECTION_NAME: &str = "io.github.kanaemi-app.inputmethod.Kanaemi_Connection";

thread_local! {
    /// The settings and the engine every client shares.
    static PROFILE: RefCell<Option<Profile>> = const { RefCell::new(None) };
    /// The Shift keys are one keyboard's, whichever client has the focus.
    static KEYS: RefCell<Keys> = RefCell::new(Keys::default());
    static CANDIDATES: RefCell<Option<Retained<IMKCandidates>>> = const { RefCell::new(None) };
    /// The controller of the field with the focus, for other programs to set
    /// the mode of.
    static ACTIVE: RefCell<Option<Retained<KanaemiController>>> = const { RefCell::new(None) };
    /// The row the candidate panel highlights.
    static PANEL_ROW: Cell<usize> = const { Cell::new(0) };
}

fn with_profile<T>(f: impl FnOnce(&mut Profile) -> T) -> T {
    PROFILE.with_borrow_mut(|profile| {
        f(profile
            .as_mut()
            .expect("the profile is opened before any controller"))
    })
}

fn new_field() -> Field {
    with_profile(|profile| Field::new(profile))
}

pub struct Ivars {
    field: RefCell<Field>,
    marked: Cell<bool>,
    shown: RefCell<Vec<String>>,
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
                shown: RefCell::new(Vec::new()),
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

        #[unsafe(method_id(candidates:))]
        fn candidates(&self, _sender: Option<&AnyObject>) -> Option<Retained<NSArray>> {
            guarded(None, || {
                let items: Vec<Retained<NSString>> =
                    self.ivars().shown.borrow().iter().map(|s| NSString::from_str(s)).collect();
                // SAFETY: an NSArray of NSString is an NSArray of objects.
                Some(unsafe { Retained::cast_unchecked(NSArray::from_retained_slice(&items)) })
            })
        }

        #[unsafe(method(candidateSelected:))]
        fn candidate_selected(&self, candidate: Option<&NSAttributedString>) {
            guarded((), || self.select(candidate));
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
        match KEYS.with_borrow_mut(|keys| keys.translate(raw)) {
            Some(key) => Bool::new(self.dispatch(Event::Key(key), sender)),
            None => Bool::NO,
        }
    }

    fn activate(&self, sender: Option<&AnyObject>) {
        ACTIVE.set(Some(self.retain()));
        secure_input::check();
        // macOS turns input methods off in a secure field, so a field the
        // IME sees is never a password field.
        self.dispatch(Event::FocusIn { password: false }, sender);
    }

    fn deactivate(&self, sender: Option<&AnyObject>) {
        self.dispatch(Event::FocusOut, sender);
        hide_candidates();
        // Another client may have been activated first.
        ACTIVE.with_borrow_mut(|active| {
            if active
                .as_deref()
                .is_some_and(|active| std::ptr::eq(active, self))
            {
                *active = None;
            }
        });
    }

    fn select(&self, candidate: Option<&NSAttributedString>) {
        let Some(candidate) = candidate else { return };
        let picked = candidate.string().to_string();
        let index = candidates::position(&self.ivars().shown.borrow(), &picked);
        if let Some(index) = index {
            let client: Option<Retained<AnyObject>> = unsafe { msg_send![self, client] };
            self.dispatch(Event::Select(index), client.as_deref());
        }
    }

    /// Feeds one event to the core and draws the result; returns whether the
    /// key was consumed. A panic clears the preedit, starts the field over and
    /// hands the key to the application.
    fn dispatch(&self, event: Event, client: Option<&AnyObject>) -> bool {
        let ivars = self.ivars();
        let handled = catch_unwind(AssertUnwindSafe(|| {
            let output = with_profile(|profile| ivars.field.borrow_mut().handle(profile, event));
            tracing::debug!(?event, ?output, "handled");
            if let Some(client) = client {
                self.draw(&output, client);
            }
            match output.send {
                // Without the permission to send keys, the pressed key goes on as
                // it is, which most applications still handle.
                Some(chord) => send_key(chord),
                None => output.consumed,
            }
        }));
        match handled {
            Ok(consumed) => consumed,
            Err(_) => {
                // The event is left out: it may be a key the user typed.
                tracing::warn!("handling an event panicked; the state was reset");
                *ivars.field.borrow_mut() = new_field();
                if let Some(client) = client {
                    set_marked_text(client, "", 0);
                }
                ivars.marked.set(false);
                ivars.shown.borrow_mut().clear();
                hide_candidates();
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
            Some(view) => {
                let shown = candidates::labels(&view.items);
                let changed = *ivars.shown.borrow() != shown;
                *ivars.shown.borrow_mut() = shown;
                show_candidates(view.selected, changed);
            }
            None => {
                ivars.shown.borrow_mut().clear();
                hide_candidates();
            }
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

fn show_candidates(selected: usize, changed: bool) {
    CANDIDATES.with_borrow(|panel| {
        let Some(panel) = panel else { return };
        unsafe {
            // Reloading moves the highlight back to the first row, so reload
            // only when the list itself changed.
            if changed || !panel.isVisible() {
                panel.updateCandidates();
                panel.show(kIMKLocateCandidatesBelowHint as NSUInteger);
                PANEL_ROW.set(0);
            }
            // selectCandidateWithIdentifier: does not move the highlight, so
            // step it the way the arrow keys would.
            let row = PANEL_ROW.get();
            let sender: Option<&AnyObject> = None;
            for _ in row..selected {
                let _: () = msg_send![&**panel, moveDown: sender];
            }
            for _ in selected..row {
                let _: () = msg_send![&**panel, moveUp: sender];
            }
            PANEL_ROW.set(selected);
        }
    });
}

fn hide_candidates() {
    CANDIDATES.with_borrow(|panel| {
        if let Some(panel) = panel
            && unsafe { panel.isVisible() }
        {
            unsafe { panel.hide() };
        }
    });
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// Posts `chord` as a key press and release; returns whether it was sent.
/// Posting needs the Accessibility permission.
fn send_key(chord: Chord) -> bool {
    let Some((code, flags)) = key_to_send(chord) else {
        tracing::warn!(?chord, "no key code to send");
        return false;
    };
    if !unsafe { AXIsProcessTrusted() } {
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
    match std::process::Command::new("/usr/bin/open")
        .arg(&app)
        .spawn()
    {
        // Waited for on a thread of its own, so no finished `open` lingers.
        Ok(mut open) => {
            let waiting = std::thread::Builder::new()
                .name("settings-launcher".to_owned())
                .spawn(move || open.wait());
            if let Err(error) = waiting {
                tracing::warn!(%error, "settings launcher not waited for");
            }
        }
        Err(error) => tracing::warn!(app = %app.display(), %error, "settings app not opened"),
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

    let panel = unsafe {
        IMKCandidates::initWithServer_panelType(
            IMKCandidates::alloc(mtm),
            Some(&server),
            kIMKSingleColumnScrollingCandidatePanel as NSUInteger,
        )
    };
    if let Some(panel) = &panel {
        // Let the controller see Space and digits before the panel does.
        let attributes = NSDictionary::from_retained_objects(
            &[unsafe { IMKCandidatesSendServerKeyEventFirst }],
            &[Retained::into_super(Retained::into_super(
                NSNumber::new_bool(true),
            ))],
        );
        // SAFETY: NSString keys and NSNumber values, as IMKCandidates expects.
        let attributes: Retained<NSDictionary> = unsafe { Retained::cast_unchecked(attributes) };
        unsafe { panel.setAttributes(Some(&attributes)) };
    }
    CANDIDATES.with_borrow_mut(|c| *c = panel);

    tracing::info!(version = kanaemi_core::VERSION, "kanaemi started");
    NSApplication::sharedApplication(mtm).run();
    drop(server);
}
