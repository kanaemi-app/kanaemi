//! The text input processor: the COM object TSF creates in each thread of
//! an application that takes text input. It turns key callbacks into core
//! events and shows the core's output as a composition.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use kanaemi_core::{Chord, Event, Key, Mode, Modifiers, Output};
use kanaemi_runtime::{Access, Field, Profile};
use windows::Win32::Foundation::*;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::TextServices::*;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, EnumThreadWindows, GA_ROOT, GUITHREADINFO, GetAncestor, GetClassNameW,
    GetGUIThreadInfo, GetMessageExtraInfo, GetWindowRect, GetWindowThreadProcessId, HHOOK,
    IsWindowVisible, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE, WM_LBUTTONDOWN,
    WM_MBUTTONDOWN, WM_NCLBUTTONDOWN, WM_NCMBUTTONDOWN, WM_NCRBUTTONDOWN, WM_RBUTTONDOWN,
};
use windows::core::*;

use crate::control::Tracker;
use crate::focus::{self, Change};
use crate::keys::{self, Keys, RawKey};
use crate::per_thread::PerThread;
use crate::pipe::{self, PipeSink};
use crate::ui_element::{CandidateList, Listed};
use crate::{candidates, indicator, input_scope, remote};

thread_local! {
    /// The settings and the engine every field of this thread shares, while
    /// a text service of the thread is active. TSF calls a text input
    /// processor only on the thread that created it.
    static PROFILE: RefCell<PerThread<Profile>> = const { RefCell::new(PerThread::new()) };
    /// The hook told of mouse buttons in the thread's windows, while active.
    static MOUSE_HOOK: Cell<Option<HHOOK>> = const { Cell::new(None) };
    /// Whether a mouse button went down since the core last heard.
    static CLICKED: Cell<bool> = const { Cell::new(false) };
}

/// Notes a mouse button going down in the thread's windows, which may move
/// the caret: the core hears of it before the next event, as a call from
/// here could come inside one from TSF.
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0
        && matches!(
            wparam.0 as u32,
            WM_LBUTTONDOWN
                | WM_RBUTTONDOWN
                | WM_MBUTTONDOWN
                | WM_NCLBUTTONDOWN
                | WM_NCRBUTTONDOWN
                | WM_NCMBUTTONDOWN
        )
    {
        CLICKED.set(true);
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Watches the mouse buttons in the thread's windows; watching them asks
/// for no permission.
fn watch_clicks() {
    if MOUSE_HOOK.get().is_some() {
        return;
    }
    match unsafe { SetWindowsHookExW(WH_MOUSE, Some(mouse_proc), None, GetCurrentThreadId()) } {
        Ok(hook) => MOUSE_HOOK.set(Some(hook)),
        Err(error) => tracing::warn!(%error, "clicks not watched"),
    }
}

fn stop_watching_clicks() {
    if let Some(hook) = MOUSE_HOOK.take() {
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }
}

fn with_profile<T>(f: impl FnOnce(&mut Profile) -> T) -> Option<T> {
    PROFILE.with_borrow_mut(|profile| profile.get_or_open(open_profile).map(f))
}

fn open_profile() -> Profile {
    let dir = kanaemi_config::dir().unwrap_or_else(|| {
        tracing::warn!("APPDATA is not set; no settings or dictionary is kept");
        std::env::temp_dir().join("kanaemi")
    });
    let access = if pipe::in_app_container() {
        Access::Sandboxed(|| Box::new(PipeSink))
    } else {
        Access::Full
    };
    let profile = Profile::open_with(dir, access);
    tracing::debug!("profile opened");
    profile
}

/// Runs a TSF callback: a panic must not unwind into the application, which
/// would end it.
fn guarded<T>(fallback: T, callback: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(callback)).unwrap_or_else(|_| {
        tracing::warn!("a text service callback panicked");
        fallback
    })
}

/// Everything one text service keeps between callbacks.
#[derive(Default)]
struct State {
    thread_mgr: RefCell<Option<ITfThreadMgr>>,
    client_id: Cell<u32>,
    thread_mgr_cookie: Cell<u32>,
    /// The text service as the sink of its compositions, while active.
    sink: RefCell<Option<ITfCompositionSink>>,
    /// The composition in each document, as one may still be ending in a
    /// field the focus has left.
    compositions: RefCell<Vec<(ITfContext, ITfComposition)>>,
    /// The field with the focus, while active.
    field: RefCell<Option<Field>>,
    /// The context the field last took the focus in.
    focused: RefCell<Option<ITfContext>>,
    /// The field as other programs see it, to tell the server of changes.
    tracker: RefCell<Tracker>,
    /// The key last handled in a test callback, which applications that
    /// test a key and then let it pass never send to the real one.
    tested: Cell<Option<(usize, isize)>>,
    /// The keys down, to read a key let go as its press read.
    keys: RefCell<Keys>,
    /// The candidates the UI element lists, shared with it.
    listed: Rc<RefCell<Listed>>,
    /// The candidate list begun with the UI element manager, by its id.
    element: RefCell<Option<u32>>,
    /// Edits not yet made, with the document each is for, oldest first.
    edits: RefCell<VecDeque<(ITfContext, Edit)>>,
    /// Backspaces sent to erase text and not yet let go: once the last is,
    /// the application has deleted with each of them.
    erasing: Cell<usize>,
    /// When those Backspaces were sent, in `GetTickCount64` milliseconds.
    erasing_since: Cell<u64>,
}

/// How long the Backspaces erasing text are waited for. Past it, the next
/// event tells the core they never came, so keys stop waiting on them.
const ERASE_WAIT_MS: u64 = 1000;

#[implement(
    ITfTextInputProcessorEx,
    ITfTextInputProcessor,
    ITfKeyEventSink,
    ITfCompositionSink,
    ITfThreadMgrEventSink,
    ITfFnConfigure,
    ITfFunction
)]
pub struct TextService {
    state: Rc<State>,
}

impl TextService {
    pub fn new() -> Self {
        Self {
            state: Rc::new(State::default()),
        }
    }
}

/// What an edit session writes into the document and shows beside it.
struct Edit {
    commit: Option<String>,
    preedit: String,
    /// The page of candidates to list, with the selected one.
    candidates: Option<(Vec<String>, usize)>,
    /// The mode to show near the caret, after it changed.
    indicator: Option<Mode>,
}

#[implement(ITfEditSession)]
struct Session {
    state: Rc<State>,
    context: ITfContext,
    sink: ITfCompositionSink,
}

impl Session {
    fn set_text(&self, ec: u32, text: &str) -> Result<()> {
        let composition = match self.state.composition(&self.context) {
            Some(composition) => composition,
            None => {
                let insert: ITfInsertAtSelection = self.context.cast()?;
                let range = unsafe { insert.InsertTextAtSelection(ec, TF_IAS_QUERYONLY, &[])? };
                let compose: ITfContextComposition = self.context.cast()?;
                let composition = unsafe { compose.StartComposition(ec, &range, &self.sink)? };
                self.state
                    .compositions
                    .borrow_mut()
                    .push((self.context.clone(), composition.clone()));
                composition
            }
        };
        let range = unsafe { composition.GetRange()? };
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe { range.SetText(ec, 0, &wide)? };
        // The caret stays at the end: some applications draw no caret
        // inside a composition.
        caret_at_end(&self.context, ec, &range)
    }

    fn end(&self, ec: u32) -> Result<()> {
        if let Some(composition) = self.state.take_composition(|(c, _)| *c == self.context) {
            unsafe { composition.EndComposition(ec)? };
        }
        Ok(())
    }

    /// Where `range` is on the screen. Before the application has laid it
    /// out, there is no answer.
    fn rect_of(&self, ec: u32, range: &ITfRange) -> Option<RECT> {
        let view = unsafe { self.context.GetActiveView() }.ok()?;
        let mut rect = RECT::default();
        let mut clipped = FALSE;
        unsafe { view.GetTextExt(ec, range, &mut rect, &mut clipped) }.ok()?;
        (rect.bottom > rect.top).then_some(rect)
    }

    /// Where the composition is, or else the caret.
    fn text_rect(&self, ec: u32) -> Option<RECT> {
        if let Some(composition) = self.state.composition(&self.context) {
            let range = unsafe { composition.GetRange() }.ok()?;
            return self.rect_of(ec, &range);
        }
        let mut selection = [TF_SELECTION::default()];
        let mut fetched = 0;
        unsafe {
            self.context
                .GetSelection(ec, TF_DEFAULT_SELECTION, &mut selection, &mut fetched)
        }
        .ok()?;
        let range = std::mem::ManuallyDrop::into_inner(std::mem::take(&mut selection[0].range))?;
        self.rect_of(ec, &range)
    }

    /// The application's top-level window the composition is in, to own the
    /// popups. The view may not say, as in the Start menu's search box; then
    /// the window with the keyboard focus. Only a window of this process can
    /// own them.
    fn owner(&self) -> Option<HWND> {
        let viewed = unsafe { self.context.GetActiveView() }
            .and_then(|view| unsafe { view.GetWnd() })
            .ok();
        let focused = Some(unsafe { GetFocus() });
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let (gui_focus, gui_active) =
            match unsafe { GetGUIThreadInfo(GetCurrentThreadId(), &mut gui) } {
                Ok(()) => (Some(gui.hwndFocus), Some(gui.hwndActive)),
                Err(_) => (None, None),
            };
        [
            viewed,
            focused,
            gui_focus,
            gui_active,
            shown_window_of_this_thread(),
        ]
        .into_iter()
        .flatten()
        .find_map(|window| {
            if window.is_invalid() {
                return None;
            }
            let root = Some(unsafe { GetAncestor(window, GA_ROOT) })
                .filter(|r| !r.is_invalid())
                .unwrap_or(window);
            let mut process = 0;
            unsafe { GetWindowThreadProcessId(root, Some(&mut process)) };
            (process == std::process::id()).then_some(root)
        })
    }

    /// Where to show something by the text: below it, or else at the
    /// window's top corner.
    fn place(&self, ec: u32, owner: Option<HWND>) -> RECT {
        self.text_rect(ec)
            .or_else(|| {
                let mut rect = RECT::default();
                unsafe { GetWindowRect(owner?, &mut rect) }.ok()?;
                Some(RECT {
                    bottom: rect.top,
                    ..rect
                })
            })
            .unwrap_or_default()
    }

    /// Begins the candidate list as a UI element, or updates it; the
    /// application answers whether the text service shows it.
    fn list(&self) {
        let Some(manager) = self.state.element_manager() else {
            self.state.listed.borrow_mut().shown = true;
            return;
        };
        if let Some(id) = *self.state.element.borrow() {
            let _ = unsafe { manager.UpdateUIElement(id) };
            return;
        }
        let Ok(document) = (unsafe { self.context.GetDocumentMgr() }) else {
            self.state.listed.borrow_mut().shown = true;
            return;
        };
        let element: ITfUIElement = CandidateList::new(self.state.listed.clone(), document).into();
        let mut show = TRUE;
        let mut id = 0;
        let shown = match unsafe { manager.BeginUIElement(&element, &mut show, &mut id) } {
            Ok(()) => {
                *self.state.element.borrow_mut() = Some(id);
                show.as_bool()
            }
            Err(_) => true,
        };
        self.state.listed.borrow_mut().shown = shown;
    }

    fn apply(&self, ec: u32, edit: Edit) -> Result<()> {
        if let Some(text) = edit.commit {
            self.set_text(ec, &text)?;
            self.end(ec)?;
        }
        if !edit.preedit.is_empty() {
            self.set_text(ec, &edit.preedit)?;
        } else if self.state.composition(&self.context).is_some() {
            self.set_text(ec, "")?;
            self.end(ec)?;
        }
        let owner = self.owner();
        match edit.candidates {
            Some((items, selected)) => {
                let at = self.place(ec, owner);
                {
                    let mut listed = self.state.listed.borrow_mut();
                    listed.items = items;
                    listed.selected = selected;
                    listed.at = at;
                    listed.owner = owner;
                }
                self.list();
                self.state.listed.borrow().draw();
            }
            None => self.state.end_list(),
        }
        if let Some(mode) = edit.indicator {
            indicator::show(mode, self.place(ec, owner), owner);
        }
        Ok(())
    }
}

impl ITfEditSession_Impl for Session_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        // Every edit waiting for this document, in the order they came: a
        // synchronous session runs ahead of an asynchronous one still
        // pending, and would otherwise put its text before the earlier.
        // Those of other documents wait for sessions of their own.
        guarded(Err(E_FAIL.into()), || {
            let mine: Vec<Edit> = {
                let mut edits = self.state.edits.borrow_mut();
                let (mine, others) = edits
                    .drain(..)
                    .partition::<Vec<_>, _>(|(context, _)| *context == self.context);
                edits.extend(others);
                mine.into_iter().map(|(_, edit)| edit).collect()
            };
            for edit in mine {
                self.apply(ec, edit)?;
            }
            Ok(())
        })
    }
}

/// Reads whether the field at `context` asks not to be recorded, from its
/// input scopes, which are read only in an edit session.
#[implement(ITfEditSession)]
struct ScopeSession {
    state: Rc<State>,
    context: ITfContext,
}

impl ITfEditSession_Impl for ScopeSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        guarded(Err(E_FAIL.into()), || {
            // The focus may have gone elsewhere before the session ran.
            if self.state.focused.borrow().as_ref() != Some(&self.context) {
                return Ok(());
            }
            let private = input_scope::asks_for_no_record(&input_scope::of(&self.context, ec));
            if let Some(field) = self.state.field.borrow_mut().as_mut() {
                field.set_private(private);
            }
            Ok(())
        })
    }
}

impl State {
    fn composition(&self, context: &ITfContext) -> Option<ITfComposition> {
        self.compositions
            .borrow()
            .iter()
            .find(|(c, _)| c == context)
            .map(|(_, composition)| composition.clone())
    }

    fn take_composition(
        &self,
        which: impl Fn(&(ITfContext, ITfComposition)) -> bool,
    ) -> Option<ITfComposition> {
        let mut compositions = self.compositions.borrow_mut();
        let index = compositions.iter().position(which)?;
        Some(compositions.remove(index).1)
    }

    /// Feeds `event` to the field, if there is one, and tells the server
    /// what changed. A panic starts the field over: the output then clears
    /// the composition and the candidates, and hands the key to the
    /// application.
    fn handle(&self, event: Event) -> Option<Output> {
        let output = {
            let mut field = self.field.borrow_mut();
            let field = field.as_mut()?;
            with_profile(|profile| {
                let handled = catch_unwind(AssertUnwindSafe(|| {
                    if CLICKED.take() {
                        field.handle(profile, Event::CaretMoved);
                    }
                    field.handle(profile, event)
                }));
                handled.unwrap_or_else(|_| {
                    // The event is left out: it may be a key the user typed.
                    tracing::warn!("handling an event panicked; the state was reset");
                    field.restart(profile)
                })
            })?
        };
        let (thread, now) = unsafe { (GetCurrentThreadId(), GetTickCount64()) };
        let report = self
            .tracker
            .borrow_mut()
            .follow(thread, event, output.mode, now);
        if let Some(report) = report {
            remote::report(report);
        }
        Some(output)
    }

    /// Feeds one event to the core and shows the result in `context`;
    /// returns whether the key was consumed.
    fn dispatch(self: &Rc<Self>, event: Event, context: Option<&ITfContext>, sync: bool) -> bool {
        // A key-up the key sink never saw would keep the core waiting.
        if self.erasing.get() > 0
            && unsafe { GetTickCount64() }.saturating_sub(self.erasing_since.get()) > ERASE_WAIT_MS
        {
            tracing::warn!("the keys erasing text never came back");
            self.erasing.set(0);
            self.dispatch(Event::Erased(false), context, sync);
        }
        let Some(output) = self.handle(event) else {
            return false;
        };
        tracing::debug!(?event, ?output, "handled");
        let erasing = output
            .erase
            .as_deref()
            .is_none_or(|text| self.erase(text, context));
        if let Some(context) = context {
            self.show(&output, context, sync);
        }
        match output.send {
            Some(chord) => send_key(chord),
            None => output.consumed && erasing,
        }
    }

    /// Sends a Backspace for each grapheme of `text`; the core hears whether
    /// it is gone once the application has handled them. Returns whether they
    /// were sent; the key that asked for them goes on as it is when not.
    fn erase(self: &Rc<Self>, text: &str, context: Option<&ITfContext>) -> bool {
        let presses = kanaemi_runtime::backspaces(text);
        let backspace = Chord {
            key: Key::Backspace,
            mods: Modifiers::default(),
        };
        if (0..presses).all(|_| send_key(backspace)) {
            self.erasing.set(presses);
            self.erasing_since.set(unsafe { GetTickCount64() });
            return true;
        }
        self.dispatch(Event::Erased(false), context, false);
        false
    }

    /// Takes a key the text service sent; once the last Backspace sent to
    /// erase text is let go, the core hears the text is gone. A key goes up
    /// only after the application handled its press.
    fn sent_key_up(self: &Rc<Self>, wparam: WPARAM, context: Option<&ITfContext>) {
        if wparam.0 != usize::from(keys::VK_BACK) || self.erasing.get() == 0 {
            return;
        }
        self.erasing.set(self.erasing.get() - 1);
        if self.erasing.get() == 0 {
            self.dispatch(Event::Erased(true), context, false);
        }
    }

    /// Whether the core would consume `event`, without changing its state.
    fn would_consume(&self, event: Event) -> bool {
        // A click since is heard first, as the key would hear it.
        if CLICKED.take() {
            self.handle(Event::CaretMoved);
        }
        let field = self.field.borrow();
        let Some(field) = field.as_ref() else {
            return false;
        };
        match catch_unwind(AssertUnwindSafe(|| field.preview(event))) {
            Ok(output) => output.consumed || output.commit.is_some() || output.send.is_some(),
            Err(_) => {
                // Said not to be eaten, the key is handled by the test,
                // where a field that panics again starts over in `handle`.
                // Unwinding would skip that, and the field would never
                // start over in applications that send only keys the test
                // eats.
                tracing::warn!("trying an event panicked");
                false
            }
        }
    }

    fn show(self: &Rc<Self>, output: &Output, context: &ITfContext, sync: bool) {
        let composing = self.composition(context).is_some();
        // An edit still waiting may show text the core has since taken back.
        let pending = self.edits.borrow().iter().any(|(c, _)| c == context);
        let writes = output.commit.is_some() || !output.preedit.is_empty() || composing || pending;
        if !writes {
            self.end_list();
        }
        if !writes && output.indicator.is_none() {
            return;
        }
        let Some(sink) = self.sink.borrow().clone() else {
            return;
        };
        let edit = Edit {
            commit: output.commit.clone(),
            preedit: output.preedit.clone(),
            candidates: output.candidates.as_ref().map(|view| {
                let items = view.items.iter().map(|c| c.surface.clone()).collect();
                (items, view.selected)
            }),
            indicator: output.indicator,
        };
        self.edits.borrow_mut().push_back((context.clone(), edit));
        let session: ITfEditSession = Session {
            state: self.clone(),
            context: context.clone(),
            sink,
        }
        .into();
        let flags = if sync {
            TF_ES_SYNC | TF_ES_READWRITE
        } else {
            TF_ES_ASYNCDONTCARE | TF_ES_READWRITE
        };
        let request =
            |flags| unsafe { context.RequestEditSession(self.client_id.get(), &session, flags) };
        let mut result = request(flags);
        // Some applications refuse to edit synchronously while handling a
        // key; the edit then runs as soon as they can, still before the next.
        if sync && result == Ok(TF_E_SYNCHRONOUS) {
            result = request(TF_ES_ASYNCDONTCARE | TF_ES_READWRITE);
        }
        let failure = match result {
            Ok(outcome) => outcome.is_err().then(|| Error::from(outcome)),
            Err(error) => Some(error),
        };
        if let Some(error) = failure {
            // A session that ran took its edit; one that never will must not
            // leave it to be made later, after other text.
            let mut edits = self.edits.borrow_mut();
            if edits.back().is_some_and(|(c, _)| c == context) {
                edits.pop_back();
            }
            tracing::warn!(%error, "edit session failed");
        }
    }

    fn key(&self, wparam: WPARAM, lparam: LPARAM, down: bool) -> Option<Event> {
        let raw = raw_key(wparam, lparam, down);
        self.keys.borrow_mut().translate(&raw).map(Event::Key)
    }

    /// The context of the field with the focus, to commit into.
    fn focused_context(&self) -> Option<ITfContext> {
        let thread_mgr = self.thread_mgr.borrow().clone()?;
        let document = unsafe { thread_mgr.GetFocus() }.ok()?;
        unsafe { document.GetTop() }.ok()
    }

    /// The field reads what changed in the settings folder as the focus
    /// comes in, so it is made only once per activation.
    fn focus_in(self: &Rc<Self>, context: Option<&ITfContext>) {
        *self.focused.borrow_mut() = context.cloned();
        if self.field.borrow().is_none() {
            *self.field.borrow_mut() = with_profile(|profile| Field::new(profile));
            // The text service runs inside the application it serves.
            let exe = std::env::current_exe().ok();
            if let Some(field) = self.field.borrow_mut().as_mut()
                && let Some(name) = exe.as_deref().and_then(|exe| exe.file_name())
            {
                field.set_application(name.to_string_lossy());
            }
        }
        self.read_privacy(context);
        // Windows turns input methods off in a password field, so a field
        // the text service sees is never one. The context shows the switch
        // to the mode a field starts in.
        self.dispatch(Event::FocusIn { password: false }, context, false);
    }

    /// Marks the field as one that asks not to be recorded when the input
    /// scopes of `context` say so. Until they are read, which the
    /// application may let happen only later, the field is taken to ask it,
    /// so nothing typed there meanwhile is recorded.
    fn read_privacy(self: &Rc<Self>, context: Option<&ITfContext>) {
        let set = |private| {
            if let Some(field) = self.field.borrow_mut().as_mut() {
                field.set_private(private);
            }
        };
        let Some(context) = context else {
            set(false);
            return;
        };
        set(true);
        let session: ITfEditSession = ScopeSession {
            state: self.clone(),
            context: context.clone(),
        }
        .into();
        let flags = TF_ES_ASYNCDONTCARE | TF_ES_READ;
        let result = unsafe { context.RequestEditSession(self.client_id.get(), &session, flags) };
        let failure = match result {
            Ok(outcome) => outcome.is_err().then(|| Error::from(outcome)),
            Err(error) => Some(error),
        };
        if let Some(error) = failure {
            tracing::warn!(%error, "the input scopes could not be read");
            set(false);
        }
    }

    /// The focus leaves `context`; the core forgets what it was erasing.
    fn focus_out(self: &Rc<Self>, context: Option<&ITfContext>) {
        self.erasing.set(0);
        *self.focused.borrow_mut() = None;
        self.dispatch(Event::FocusOut, context, false);
        indicator::hide();
    }

    /// A context was pushed onto a document's stack, or `popped` off it.
    /// Applications such as Word cover the document with a context of their
    /// own for a while: when the top of the focused document changes, what
    /// is visible is committed in the context it was typed in, and the field
    /// goes on in the new top in the mode it was in.
    fn stack_changed(self: &Rc<Self>, popped: Option<&ITfContext>) {
        let document = self
            .thread_mgr
            .borrow()
            .as_ref()
            .and_then(|thread_mgr| unsafe { thread_mgr.GetFocus() }.ok());
        let stack = document
            .map(|document| unsafe { [document.GetTop().ok(), document.GetBase().ok()] })
            .into_iter()
            .flatten()
            .flatten();
        let held = self.focused.borrow().clone();
        match focus::change(stack, popped, held) {
            Change::Stays => {}
            Change::Comes(context) => self.focus_in(Some(&context)),
            Change::Goes(context) => self.focus_out(Some(&context)),
            Change::Passes { from, to } => {
                self.erasing.set(0);
                self.dispatch(Event::Flush, Some(&from), false);
                *self.focused.borrow_mut() = Some(to);
            }
        }
    }

    fn element_manager(&self) -> Option<ITfUIElementMgr> {
        self.thread_mgr.borrow().as_ref()?.cast().ok()
    }

    /// Ends the candidate list, and hides it wherever it shows.
    fn end_list(&self) {
        candidates::hide();
        let Some(id) = self.element.borrow_mut().take() else {
            return;
        };
        if let Some(manager) = self.element_manager() {
            let _ = unsafe { manager.EndUIElement(id) };
        }
    }
}

/// Puts the caret at the end of `range`.
fn caret_at_end(context: &ITfContext, ec: u32, range: &ITfRange) -> Result<()> {
    let caret = unsafe { range.Clone()? };
    unsafe { caret.Collapse(ec, TF_ANCHOR_END)? };
    let mut selection = [TF_SELECTION {
        range: std::mem::ManuallyDrop::new(Some(caret)),
        style: TF_SELECTIONSTYLE {
            ase: TF_AE_NONE,
            fInterimChar: FALSE,
        },
    }];
    let set = unsafe { context.SetSelection(ec, &selection) };
    // The selection holds its range by hand; it is let go whatever the
    // outcome.
    unsafe { std::mem::ManuallyDrop::drop(&mut selection[0].range) };
    set
}

/// Whether the selection lies within `range`, ends included.
fn selection_within(context: &ITfContext, ec: u32, range: &ITfRange) -> bool {
    let mut selection = [TF_SELECTION::default()];
    let mut fetched = 0;
    if unsafe { context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selection, &mut fetched) }
        .is_err()
        || fetched == 0
    {
        return false;
    }
    let Some(selected) =
        std::mem::ManuallyDrop::into_inner(std::mem::take(&mut selection[0].range))
    else {
        return false;
    };
    let start = unsafe { selected.CompareStart(ec, range, TF_ANCHOR_START) };
    let end = unsafe { selected.CompareEnd(ec, range, TF_ANCHOR_END) };
    matches!((start, end), (Ok(start), Ok(end)) if start >= 0 && end <= 0)
}

/// A top-level window of this thread that shows and is not the text
/// service's own: in the Start menu's search box, the CoreWindow, which no
/// other call names.
fn shown_window_of_this_thread() -> Option<HWND> {
    extern "system" fn found(window: HWND, out: LPARAM) -> BOOL {
        let out = unsafe { &mut *(out.0 as *mut Option<HWND>) };
        let mut class = [0u16; 64];
        let len = unsafe { GetClassNameW(window, &mut class) } as usize;
        let ours = String::from_utf16_lossy(&class[..len]).starts_with("Kanaemi");
        if !ours && unsafe { IsWindowVisible(window) }.as_bool() {
            *out = Some(window);
            return FALSE;
        }
        TRUE
    }
    let mut out: Option<HWND> = None;
    unsafe {
        let _ = EnumThreadWindows(
            GetCurrentThreadId(),
            Some(found),
            LPARAM(&mut out as *mut Option<HWND> as isize),
        );
    }
    out
}

fn raw_key(wparam: WPARAM, lparam: LPARAM, down: bool) -> RawKey {
    let vk = wparam.0 as u16;
    let scan = ((lparam.0 >> 16) & 0xff) as u16;
    let held = |vk: u16| unsafe { GetKeyState(i32::from(vk)) } < 0;
    let mods = kanaemi_core::Modifiers {
        shift: held(keys::VK_SHIFT),
        ctrl: held(keys::VK_CONTROL),
        alt: held(keys::VK_MENU),
        cmd: held(keys::VK_LWIN) || held(keys::VK_RWIN),
    };
    RawKey {
        vk,
        scan,
        extended: (lparam.0 >> 24) & 1 == 1,
        down,
        // The key's state before this message: down already means a repeat.
        repeat: down && (lparam.0 >> 30) & 1 == 1,
        mods,
        character: character(vk, scan, mods.shift),
        time_ms: unsafe { GetTickCount64() },
    }
}

/// What the key types with Shift and Caps Lock alone applied, so Ctrl does
/// not turn it into a control character.
fn character(vk: u16, scan: u16, shift: bool) -> Option<char> {
    let mut state = [0u8; 256];
    if shift {
        state[usize::from(keys::VK_SHIFT)] = 0x80;
    }
    // Caps Lock turned on types capitals, as the application would show.
    if unsafe { GetKeyState(i32::from(keys::VK_CAPITAL)) } & 1 == 1 {
        state[usize::from(keys::VK_CAPITAL)] = 0x01;
    }
    let mut buffer = [0u16; 4];
    // Flag 4 leaves the keyboard's dead-key state as it is.
    let written =
        unsafe { ToUnicode(u32::from(vk), u32::from(scan), Some(&state), &mut buffer, 4) };
    if written != 1 {
        return None;
    }
    char::from_u32(u32::from(buffer[0]))
}

/// Marks the keys the text service sends, so they pass to the application
/// as they are: put through the bindings again, a key bound to itself, or two
/// keys bound to each other, would be sent without end.
const SENT: usize = 0x4b41_4e41;

/// Whether the key being handled is one the text service sent.
fn sent_here() -> bool {
    unsafe { GetMessageExtraInfo() }.0 as usize == SENT
}

/// Sends `chord` to the application in place of the key pressed; returns
/// whether it was sent. Modifiers held for the key pressed are let go for
/// the key sent, and pressed again after it.
fn send_key(chord: Chord) -> bool {
    let Some(vk) = keys::key_to_send(chord) else {
        tracing::warn!(?chord, "no key code to send");
        return false;
    };
    let input = |vk: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: {
                    let mut flags = KEYBD_EVENT_FLAGS(0);
                    if up {
                        flags |= KEYEVENTF_KEYUP;
                    }
                    // Without it, a right-hand modifier is the left one and an
                    // arrow is the number pad's.
                    if keys::is_extended(vk) {
                        flags |= KEYEVENTF_EXTENDEDKEY;
                    }
                    flags
                },
                time: 0,
                dwExtraInfo: SENT,
            },
        },
    };
    // Each side on its own: a key held on the right is let go on the right.
    let modifiers = [
        (keys::VK_LSHIFT, keys::VK_RSHIFT, chord.mods.shift),
        (keys::VK_LCONTROL, keys::VK_RCONTROL, chord.mods.ctrl),
        (keys::VK_LMENU, keys::VK_RMENU, chord.mods.alt),
        (keys::VK_LWIN, keys::VK_RWIN, chord.mods.cmd),
    ];
    let down = |vk: u16| unsafe { GetKeyState(i32::from(vk)) } < 0;
    // (key, whether it is held): a held key the chord lacks goes up, and a
    // key the chord needs goes down when neither side is held.
    let held: Vec<(u16, bool)> = modifiers
        .iter()
        .flat_map(
            |&(left, right, wanted)| match (wanted, down(left) || down(right)) {
                (false, true) => [left, right]
                    .into_iter()
                    .filter(|&vk| down(vk))
                    .map(|vk| (vk, true))
                    .collect(),
                (true, false) => vec![(left, false)],
                _ => Vec::new(),
            },
        )
        .collect();
    let mut inputs = Vec::new();
    inputs.extend(held.iter().map(|&(vk, held)| input(vk, held)));
    inputs.push(input(vk, false));
    inputs.push(input(vk, true));
    inputs.extend(held.iter().map(|&(vk, held)| input(vk, !held)));
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    sent as usize == inputs.len()
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<ITfThreadMgr>, tid: u32) -> Result<()> {
        ITfTextInputProcessorEx_Impl::ActivateEx(self, ptim, tid, 0)
    }

    fn Deactivate(&self) -> Result<()> {
        guarded(Ok(()), || {
            let state = &self.state;
            let context = state.focused_context();
            // The focus going writes the picks not yet written.
            state.dispatch(Event::FocusOut, context.as_ref(), false);
            state.end_list();
            indicator::hide();
            let thread_mgr = state.thread_mgr.borrow_mut().take();
            if let Some(thread_mgr) = &thread_mgr {
                if let Ok(keystrokes) = thread_mgr.cast::<ITfKeystrokeMgr>() {
                    let _ = unsafe { keystrokes.UnadviseKeyEventSink(state.client_id.get()) };
                }
                if let Ok(source) = thread_mgr.cast::<ITfSource>() {
                    let _ = unsafe { source.UnadviseSink(state.thread_mgr_cookie.get()) };
                }
            }
            // The sink is the text service itself: holding it longer would
            // keep it alive.
            *state.sink.borrow_mut() = None;
            *state.field.borrow_mut() = None;
            *state.focused.borrow_mut() = None;
            remote::detach();
            stop_watching_clicks();
            // Dropped here, outside the loader lock a thread-local is dropped
            // under, once no text service of the thread is active.
            if thread_mgr.is_some()
                && let Some(profile) = PROFILE.with_borrow_mut(PerThread::deactivate)
            {
                drop(profile);
                tracing::debug!("profile released");
            }
            Ok(())
        })
    }
}

impl ITfTextInputProcessorEx_Impl for TextService_Impl {
    fn ActivateEx(&self, ptim: Ref<ITfThreadMgr>, tid: u32, _flags: u32) -> Result<()> {
        guarded(Err(E_FAIL.into()), || {
            crate::init_logging();
            let state = &self.state;
            let thread_mgr = ptim.ok()?.clone();
            let keystrokes: ITfKeystrokeMgr = thread_mgr.cast()?;
            let source: ITfSource = thread_mgr.cast()?;
            let key_sink: ITfKeyEventSink = self.to_interface();
            unsafe { keystrokes.AdviseKeyEventSink(tid, &key_sink, true)? };
            let focus_sink: ITfThreadMgrEventSink = self.to_interface();
            let cookie =
                match unsafe { source.AdviseSink(&ITfThreadMgrEventSink::IID, &focus_sink) } {
                    Ok(cookie) => cookie,
                    Err(error) => {
                        // A failed activation is not followed by Deactivate, so
                        // nothing advised may stay behind.
                        let _ = unsafe { keystrokes.UnadviseKeyEventSink(tid) };
                        return Err(error);
                    }
                };
            state.client_id.set(tid);
            // Held only once active: the sink is the text service itself.
            *state.sink.borrow_mut() = Some(self.to_interface());
            state.thread_mgr_cookie.set(cookie);
            if state.thread_mgr.borrow_mut().replace(thread_mgr).is_none() {
                PROFILE.with_borrow_mut(PerThread::activate);
            }
            candidates::on_pick({
                let state = Rc::downgrade(&self.state);
                move |row| {
                    if let Some(state) = state.upgrade() {
                        let context = state.focused_context();
                        state.dispatch(Event::Select(row), context.as_ref(), false);
                    }
                }
            });
            remote::attach({
                let state = Rc::downgrade(&self.state);
                move |mode| {
                    // A field the focus has left is not the one the server
                    // meant.
                    if let Some(state) = state.upgrade()
                        && state.tracker.borrow().focused()
                    {
                        let context = state.focused_context();
                        state.dispatch(Event::SetMode(mode), context.as_ref(), false);
                    }
                }
            });
            watch_clicks();
            let context = state.focused_context();
            state.focus_in(context.as_ref());
            Ok(())
        })
    }
}

impl ITfCompositionSink_Impl for TextService_Impl {
    /// The application ended the composition, as on a click elsewhere: the
    /// text it keeps is what would be committed, without the marks.
    fn OnCompositionTerminated(&self, ec: u32, composition: Ref<ITfComposition>) -> Result<()> {
        guarded(Ok(()), || {
            let state = &self.state;
            if let Some(ended) = composition.as_ref() {
                let ended_in = state
                    .compositions
                    .borrow()
                    .iter()
                    .find(|(_, c)| c == ended)
                    .map(|(context, _)| context.clone());
                state.take_composition(|(_, c)| c == ended);
                // A field the focus has left was flushed then; the field with
                // the focus now keeps what is typed in it.
                if ended_in.is_some_and(|context| Some(context) != state.focused_context()) {
                    return Ok(());
                }
            }
            state.end_list();
            let Some(output) = state.handle(Event::Flush) else {
                return Ok(());
            };
            if let (Some(text), Some(composition)) = (output.commit, composition.as_ref()) {
                let range = unsafe { composition.GetRange()? };
                let context = unsafe { range.GetContext()? };
                // Notepad ends the composition as it goes to the background,
                // with the caret still in it, and the new text puts the caret
                // at its start: typing on coming back would go before it. A
                // caret the click that ended it put elsewhere stays there.
                let caret_inside = selection_within(&context, ec, &range);
                let wide: Vec<u16> = text.encode_utf16().collect();
                unsafe { range.SetText(ec, 0, &wide)? };
                if caret_inside {
                    caret_at_end(&context, ec, &range)?;
                }
            }
            Ok(())
        })
    }
}

impl ITfThreadMgrEventSink_Impl for TextService_Impl {
    fn OnInitDocumentMgr(&self, _document: Ref<ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }

    fn OnUninitDocumentMgr(&self, _document: Ref<ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }

    fn OnSetFocus(&self, focus: Ref<ITfDocumentMgr>, previous: Ref<ITfDocumentMgr>) -> Result<()> {
        guarded(Ok(()), || {
            let previous = previous.as_ref().and_then(|d| unsafe { d.GetTop() }.ok());
            self.state.focus_out(previous.as_ref());
            if let Some(focus) = focus.as_ref() {
                let context = unsafe { focus.GetTop() }.ok();
                self.state.focus_in(context.as_ref());
            }
            Ok(())
        })
    }

    fn OnPushContext(&self, _context: Ref<ITfContext>) -> Result<()> {
        guarded(Ok(()), || {
            self.state.stack_changed(None);
            Ok(())
        })
    }

    fn OnPopContext(&self, context: Ref<ITfContext>) -> Result<()> {
        guarded(Ok(()), || {
            self.state.stack_changed(context.as_ref());
            Ok(())
        })
    }
}

impl ITfKeyEventSink_Impl for TextService_Impl {
    /// The application went to the background or came back: the popups,
    /// topmost, would otherwise stay over the next application.
    fn OnSetFocus(&self, foreground: BOOL) -> Result<()> {
        guarded(Ok(()), || {
            if foreground.as_bool() {
                if self.state.element.borrow().is_some() {
                    self.state.listed.borrow().draw();
                }
            } else {
                candidates::hide();
                indicator::hide();
            }
            Ok(())
        })
    }

    // Some applications test a key before sending it, and send it only when
    // the test says it is eaten; others, such as Notepad, never test. A key
    // the test lets pass is handled there, as no other call follows.
    fn OnTestKeyDown(
        &self,
        context: Ref<ITfContext>,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<BOOL> {
        guarded(Ok(FALSE), || {
            if sent_here() {
                return Ok(FALSE);
            }
            let state = &self.state;
            // A key tested and eaten comes again to OnKeyDown, which reads
            // it as this press read it.
            let Some(event) = state.key(wparam, lparam, true) else {
                return Ok(FALSE);
            };
            if state.would_consume(event) {
                return Ok(TRUE);
            }
            state.tested.set(Some((wparam.0, lparam.0)));
            state.dispatch(event, context.as_ref(), true);
            Ok(FALSE)
        })
    }

    fn OnTestKeyUp(
        &self,
        context: Ref<ITfContext>,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<BOOL> {
        guarded(Ok(FALSE), || {
            let state = &self.state;
            if sent_here() {
                // OnKeyUp may follow for the same release; it is counted once.
                state.tested.set(Some((wparam.0, lparam.0)));
                state.sent_key_up(wparam, context.as_ref());
                return Ok(FALSE);
            }
            state.tested.set(Some((wparam.0, lparam.0)));
            if let Some(event) = state.key(wparam, lparam, false) {
                state.dispatch(event, context.as_ref(), false);
            }
            Ok(FALSE)
        })
    }

    fn OnKeyDown(&self, context: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        guarded(Ok(FALSE), || {
            if sent_here() {
                return Ok(FALSE);
            }
            let state = &self.state;
            if state.tested.take() == Some((wparam.0, lparam.0)) {
                return Ok(FALSE);
            }
            let Some(event) = state.key(wparam, lparam, true) else {
                return Ok(FALSE);
            };
            Ok(state.dispatch(event, context.as_ref(), true).into())
        })
    }

    /// A release is never eaten. The application saw the press, or the core
    /// held it back and types what it meant now, as the release ends it.
    fn OnKeyUp(&self, context: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        guarded(Ok(FALSE), || {
            let state = &self.state;
            if state.tested.take() == Some((wparam.0, lparam.0)) {
                return Ok(FALSE);
            }
            if sent_here() {
                state.sent_key_up(wparam, context.as_ref());
                return Ok(FALSE);
            }
            if let Some(event) = state.key(wparam, lparam, false) {
                state.dispatch(event, context.as_ref(), false);
            }
            Ok(FALSE)
        })
    }

    fn OnPreservedKey(&self, _context: Ref<ITfContext>, _guid: *const GUID) -> Result<BOOL> {
        Ok(FALSE)
    }
}

/// The settings app opens from the input method's options in the Windows
/// settings, which create a text service of their own and ask it for
/// `ITfFnConfigure`. The language bar is left alone: it is where Windows
/// shows input modes, and the IME shows its mode itself.
impl ITfFunction_Impl for TextService_Impl {
    fn GetDisplayName(&self) -> Result<BSTR> {
        Ok(BSTR::from("設定"))
    }
}

impl ITfFnConfigure_Impl for TextService_Impl {
    fn Show(&self, _parent: HWND, _langid: u16, _profile: *const GUID) -> Result<()> {
        guarded((), open_settings_app);
        Ok(())
    }
}

/// Opens the settings app installed beside the DLL.
fn open_settings_app() {
    // The DLL for 32-bit applications sits in a folder of its own.
    let Some(folder) = crate::com::module_path().and_then(|path| {
        let folder = path.parent()?;
        Some(match folder.file_name() {
            Some(name) if name == "x86" => folder.parent()?.to_owned(),
            _ => folder.to_owned(),
        })
    }) else {
        tracing::warn!("no folder to find the settings app in");
        return;
    };
    let app = folder.join(SETTINGS_APP);
    if let Err(error) = std::process::Command::new(&app).spawn() {
        tracing::warn!(app = %app.display(), %error, "settings app not opened");
    }
}

const SETTINGS_APP: &str = "kanaemi-settings.exe";
