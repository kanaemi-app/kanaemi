//! The C interface between the C++ layer and the shell. Every call comes on
//! Fcitx5's event loop; an input context is named by its address, which
//! comes back as the context the callbacks tell.
//!
//! A panic would unwind into Fcitx5 and end it, taking every application's
//! input with it, so each call catches one and answers as if nothing were
//! done.

use std::ffi::{CStr, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use kanaemi_linux::reply::Reply;

use crate::shell::{Id, Shell};
use crate::show::{self, Candidate, Host};

/// How the C++ layer is told what to show in a context, and woken on its
/// event loop.
#[repr(C)]
pub struct Callbacks {
    commit: unsafe extern "C" fn(ic: *mut c_void, text: *const c_char, len: usize),
    /// The cursor is in bytes from the preedit's start.
    preedit: unsafe extern "C" fn(ic: *mut c_void, text: *const c_char, len: usize, cursor: usize),
    candidates: unsafe extern "C" fn(
        ic: *mut c_void,
        items: *const Item,
        count: usize,
        selected: usize,
        aside: *const c_char,
        aside_len: usize,
    ),
    hide_candidates: unsafe extern "C" fn(ic: *mut c_void),
    forward: unsafe extern "C" fn(ic: *mut c_void, keysym: u32, state: u32),
    indicator: unsafe extern "C" fn(ic: *mut c_void, label: *const c_char, len: usize),
    /// Asks for [`kanaemi_fcitx5_serve`] on the event loop; called from
    /// another thread.
    wake: unsafe extern "C" fn(engine: *mut c_void),
}

/// A candidate, its text and comment as UTF-8 that is not NUL-terminated.
#[repr(C)]
pub struct Item {
    text: *const c_char,
    text_len: usize,
    comment: *const c_char,
    comment_len: usize,
}

/// The engine to wake, until the add-on goes.
struct Waker {
    wake: unsafe extern "C" fn(*mut c_void),
    engine: *mut c_void,
}

// The C++ layer wakes its event loop from any thread.
unsafe impl Send for Waker {}

pub struct Addon {
    shell: Shell,
    callbacks: Callbacks,
    waker: Arc<Mutex<Option<Waker>>>,
    /// The file the add-on was loaded from, with its links followed.
    library: PathBuf,
}

impl Addon {
    /// Tells the context what came of an event; whether the key was used.
    fn tell(&self, ic: Id, reply: Reply) -> bool {
        let mut context = Context {
            callbacks: &self.callbacks,
            ic: ic as *mut c_void,
        };
        show::tell(&mut context, &reply.signals);
        reply.consumed
    }
}

impl Drop for Addon {
    fn drop(&mut self) {
        // The thread that wakes the engine may outlive the add-on.
        if let Ok(mut waker) = self.waker.lock() {
            *waker = None;
        }
    }
}

struct Context<'a> {
    callbacks: &'a Callbacks,
    ic: *mut c_void,
}

impl Host for Context<'_> {
    fn commit(&mut self, text: &str) {
        unsafe { (self.callbacks.commit)(self.ic, text.as_ptr().cast(), text.len()) }
    }

    fn preedit(&mut self, text: &str, cursor: usize) {
        unsafe { (self.callbacks.preedit)(self.ic, text.as_ptr().cast(), text.len(), cursor) }
    }

    fn candidates(&mut self, items: &[Candidate<'_>], selected: usize, aside: Option<&str>) {
        let items: Vec<Item> = items
            .iter()
            .map(|c| Item {
                text: c.text.as_ptr().cast(),
                text_len: c.text.len(),
                comment: c.comment.as_ptr().cast(),
                comment_len: c.comment.len(),
            })
            .collect();
        let aside = aside.unwrap_or_default();
        unsafe {
            (self.callbacks.candidates)(
                self.ic,
                items.as_ptr(),
                items.len(),
                selected,
                aside.as_ptr().cast(),
                aside.len(),
            )
        }
    }

    fn hide_candidates(&mut self) {
        unsafe { (self.callbacks.hide_candidates)(self.ic) }
    }

    fn forward(&mut self, keysym: u32, state: u32) {
        unsafe { (self.callbacks.forward)(self.ic, keysym, state) }
    }

    fn indicator(&mut self, label: &str) {
        unsafe { (self.callbacks.indicator)(self.ic, label.as_ptr().cast(), label.len()) }
    }
}

fn guarded<T>(fallback: T, call: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(call)).unwrap_or_else(|_| {
        tracing::warn!("the add-on panicked; the call was left out");
        fallback
    })
}

/// # Safety
///
/// `text` is NUL-terminated or null.
unsafe fn string<'a>(text: *const c_char) -> &'a str {
    if text.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(text) }.to_str().unwrap_or_default()
}

/// Makes the add-on, or returns null when there is no settings folder.
/// `library` is the file Fcitx5 loaded it from.
///
/// # Safety
///
/// `callbacks` points to callbacks that stay valid while the add-on lives,
/// and `library` is NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_new(
    callbacks: *const Callbacks,
    engine: *mut c_void,
    library: *const c_char,
) -> *mut Addon {
    let callbacks = unsafe { callbacks.read() };
    let library = unsafe { string(library) };
    guarded(std::ptr::null_mut(), move || {
        kanaemi_runtime::init_logging();
        let Some(dir) = kanaemi_config::dir() else {
            tracing::error!("HOME is not set; no settings or dictionary is kept");
            return std::ptr::null_mut();
        };
        let waker = Arc::new(Mutex::new(Some(Waker {
            wake: callbacks.wake,
            engine,
        })));
        let mut shell = Shell::new(dir);
        // Requests from other programs are served among the events, on
        // Fcitx5's event loop, in the order they came.
        shell.profile.listen({
            let waker = Arc::clone(&waker);
            move || {
                if let Ok(waker) = waker.lock()
                    && let Some(waker) = waker.as_ref()
                {
                    unsafe { (waker.wake)(waker.engine) }
                }
            }
        });
        let library = Path::new(library);
        let addon = Addon {
            shell,
            callbacks,
            waker,
            library: library
                .canonicalize()
                .unwrap_or_else(|_| library.to_owned()),
        };
        tracing::info!(version = kanaemi_core::VERSION, "kanaemi started");
        Box::into_raw(Box::new(addon))
    })
}

/// # Safety
///
/// `addon` came from [`kanaemi_fcitx5_new`] and is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_free(addon: *mut Addon) {
    if !addon.is_null() {
        let addon = unsafe { Box::from_raw(addon) };
        guarded((), move || drop(addon));
    }
}

/// # Safety
///
/// `addon` came from [`kanaemi_fcitx5_new`] and is used on one thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_create(addon: *mut Addon, ic: *mut c_void) {
    let addon = unsafe { &mut *addon };
    guarded((), || addon.shell.create(ic as Id));
}

/// # Safety
///
/// As for [`kanaemi_fcitx5_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_destroy(addon: *mut Addon, ic: *mut c_void) {
    let addon = unsafe { &mut *addon };
    guarded((), || addon.shell.destroy(ic as Id));
}

/// Whether the key was used.
///
/// # Safety
///
/// As for [`kanaemi_fcitx5_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_key(
    addon: *mut Addon,
    ic: *mut c_void,
    keysym: u32,
    code: u32,
    state: u32,
    release: bool,
) -> bool {
    let addon = unsafe { &mut *addon };
    guarded(false, || {
        let reply = addon.shell.key(ic as Id, keysym, code, state, release);
        addon.tell(ic as Id, reply)
    })
}

/// # Safety
///
/// As for [`kanaemi_fcitx5_create`], and `program` is NUL-terminated or
/// null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_focus_in(
    addon: *mut Addon,
    ic: *mut c_void,
    flags: u64,
    program: *const c_char,
) {
    let addon = unsafe { &mut *addon };
    let program = unsafe { string(program) };
    guarded((), || {
        let reply = addon.shell.focus_in(ic as Id, flags, program);
        addon.tell(ic as Id, reply);
    });
}

/// # Safety
///
/// As for [`kanaemi_fcitx5_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_set_capabilities(
    addon: *mut Addon,
    ic: *mut c_void,
    flags: u64,
) {
    let addon = unsafe { &mut *addon };
    guarded((), || {
        if let Some(reply) = addon.shell.set_capabilities(ic as Id, flags) {
            addon.tell(ic as Id, reply);
        }
    });
}

/// # Safety
///
/// As for [`kanaemi_fcitx5_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_focus_out(
    addon: *mut Addon,
    ic: *mut c_void,
    committable: bool,
) {
    let addon = unsafe { &mut *addon };
    guarded((), || {
        let reply = addon.shell.focus_out(ic as Id, committable);
        addon.tell(ic as Id, reply);
    });
}

/// # Safety
///
/// As for [`kanaemi_fcitx5_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_reset(addon: *mut Addon, ic: *mut c_void) {
    let addon = unsafe { &mut *addon };
    guarded((), || {
        let reply = addon.shell.reset(ic as Id);
        addon.tell(ic as Id, reply);
    });
}

/// # Safety
///
/// As for [`kanaemi_fcitx5_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_select(addon: *mut Addon, ic: *mut c_void, index: usize) {
    let addon = unsafe { &mut *addon };
    guarded((), || {
        let reply = addon.shell.select(ic as Id, index);
        addon.tell(ic as Id, reply);
    });
}

/// Serves the requests other programs sent since the last wake.
///
/// # Safety
///
/// As for [`kanaemi_fcitx5_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_serve(addon: *mut Addon) {
    let addon = unsafe { &mut *addon };
    guarded((), || {
        let Addon {
            shell, callbacks, ..
        } = addon;
        shell.serve_control(|ic, reply| {
            let mut context = Context {
                callbacks,
                ic: ic as *mut c_void,
            };
            show::tell(&mut context, &reply.signals);
        });
    });
}

/// # Safety
///
/// As for [`kanaemi_fcitx5_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kanaemi_fcitx5_open_settings(addon: *mut Addon) {
    let addon = unsafe { &*addon };
    guarded((), || kanaemi_linux::open_settings(&addon.library));
}

unsafe extern "C" {
    /// The engine's factory, which the C++ layer makes.
    fn kanaemi_fcitx5_factory() -> *mut c_void;
}

/// What Fcitx5 looks up in an add-on's library to make it. Exported from
/// here, as a Rust library exports only the symbols Rust defines.
#[unsafe(no_mangle)]
pub extern "C" fn fcitx_addon_factory_instance() -> *mut c_void {
    unsafe { kanaemi_fcitx5_factory() }
}
