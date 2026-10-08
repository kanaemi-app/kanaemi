//! The text service's end of the control pipe: it tells the server what the
//! field of each thread does, and runs the modes the server sets on that
//! thread, as TSF calls a text service only on its own thread.
//!
//! A process connects once, when one of its fields first has something to
//! report and the server serves the pipe; it serves it only while the
//! settings name a port. The connection's thread hands each mode to the
//! thread it is for through a message-only window of that thread. The
//! message says only that something waits: the mode itself stays in this
//! process, so a message another process posts sets nothing.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use kanaemi_core::Mode;
use windows::Win32::Foundation::{ERROR_PIPE_BUSY, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Storage::FileSystem::FILE_FLAG_OVERLAPPED;
use windows::Win32::System::Pipes::WaitNamedPipeW;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, w};

use crate::control::{Command, Report};
use crate::link::{self, Outbox};
use crate::pipe::{self, CONTROL_CLIENT_ACCESS};

const CLASS: PCWSTR = w!("KanaemiControl");
/// Says to the window that modes wait for its thread.
const WM_MODES_WAITING: u32 = WM_APP + 1;
/// How long a report waits for a server busy letting another process in.
const BUSY_WAIT_MS: u32 = 20;
/// How many modes wait for one thread; the server sends the next only once
/// the last was taken or given up on, so more come only from a hung thread.
const MAX_WAITING: usize = 8;

/// The connection to the server, once made.
static LINK: Mutex<Option<Arc<Outbox>>> = Mutex::new(None);
/// The threads with a text service active, and the modes waiting for each.
static THREADS: Mutex<Vec<Waiting>> = Mutex::new(Vec::new());

/// Puts the field of this thread in a mode the server set.
type SetMode = Box<dyn Fn(Mode)>;

struct Waiting {
    thread: u32,
    /// The thread's window, as its handle's value: a handle is not `Send`.
    window: isize,
    /// Each with its request and the time it is to be set by.
    modes: VecDeque<(u64, Mode, u64)>,
}

thread_local! {
    static WINDOW: RefCell<Option<HWND>> = const { RefCell::new(None) };
    static SET_MODE: RefCell<Option<SetMode>> = const { RefCell::new(None) };
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Takes the modes the server sets for this thread's field with `set_mode`
/// while the text service is active.
pub fn attach(set_mode: impl Fn(Mode) + 'static) {
    SET_MODE.set(Some(Box::new(set_mode)));
    if WINDOW.with_borrow(Option::is_some) {
        return;
    }
    let window = match create() {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(%error, "no window to take modes set from outside");
            return;
        }
    };
    WINDOW.set(Some(window));
    lock(&THREADS).push(Waiting {
        thread: unsafe { GetCurrentThreadId() },
        window: window.0 as isize,
        modes: VecDeque::new(),
    });
}

/// Stops taking modes for this thread: the text service is no longer active.
pub fn detach() {
    SET_MODE.set(None);
    let thread = unsafe { GetCurrentThreadId() };
    lock(&THREADS).retain(|waiting| waiting.thread != thread);
    if let Some(window) = WINDOW.take() {
        let _ = unsafe { DestroyWindow(window) };
    }
}

/// Tells the server `report`, connecting first if need be. Nothing waits
/// for the server: a report it cannot take is dropped.
pub fn report(report: Report) {
    let mut link = lock(&LINK);
    if link.as_ref().is_none_or(|outbox| outbox.ended()) {
        *link = connect();
    }
    if let Some(outbox) = link.as_ref() {
        outbox.send(report.encode());
    }
}

fn connect() -> Option<Arc<Outbox>> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    let name = NAME.get_or_init(|| pipe::control_name().ok()).as_deref()?;
    let open = || {
        OpenOptions::new()
            .access_mode(CONTROL_CLIENT_ACCESS)
            .custom_flags(FILE_FLAG_OVERLAPPED.0)
            .open(name)
    };
    let opened = match open() {
        Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) => {
            let _ = unsafe { WaitNamedPipeW(&HSTRING::from(name), BUSY_WAIT_MS) };
            open()
        }
        opened => opened,
    };
    // No pipe is the usual case: the settings name no port.
    let pipe = opened.ok()?;
    if !pipe::made_by_user(&pipe).unwrap_or(false) {
        tracing::warn!("the control pipe was made by another user");
        return None;
    }
    let outbox = Arc::new(Outbox::new().ok()?);
    // The connection's thread outlives the text service, which COM may
    // unload with the DLL meanwhile.
    if let Err(error) = crate::com::pin() {
        tracing::warn!(%error, "control connection not served");
        return None;
    }
    let spawned = std::thread::Builder::new()
        .name("kanaemi-control".to_owned())
        .spawn({
            let outbox = outbox.clone();
            move || link::run(pipe, &outbox, |line| hand_over(line, &outbox))
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "control connection not served");
        return None;
    }
    Some(outbox)
}

/// Hands a mode the server set to the thread it is for; one for a thread
/// that is gone is done with at once. Returns whether the line was one.
fn hand_over(line: &str, outbox: &Outbox) -> bool {
    let Some((
        Command::SetMode {
            thread,
            request,
            mode,
        },
        by,
    )) = Command::decode(line)
    else {
        return false;
    };
    let posted = {
        let mut threads = lock(&THREADS);
        threads
            .iter_mut()
            .find(|waiting| waiting.thread == thread)
            .is_some_and(|waiting| {
                if waiting.modes.len() >= MAX_WAITING {
                    waiting.modes.pop_front();
                }
                waiting.modes.push_back((request, mode, by));
                let window = HWND(waiting.window as *mut _);
                unsafe { PostMessageW(Some(window), WM_MODES_WAITING, WPARAM(0), LPARAM(0)) }
                    .is_ok()
            })
    };
    if !posted {
        outbox.send(Report::Done { thread, request }.encode());
    }
    true
}

/// Sets the modes waiting for this thread, in order. One whose time is up
/// is not set: the program that asked was answered without it.
fn set_waiting_modes() {
    let thread = unsafe { GetCurrentThreadId() };
    let modes = lock(&THREADS)
        .iter_mut()
        .find(|waiting| waiting.thread == thread)
        .map(|waiting| std::mem::take(&mut waiting.modes))
        .unwrap_or_default();
    for (request, mode, by) in modes {
        if unsafe { GetTickCount64() } < by {
            SET_MODE.with_borrow(|set_mode| {
                if let Some(set_mode) = set_mode {
                    set_mode(mode);
                }
            });
        }
        report(Report::Done { thread, request });
    }
}

fn create() -> windows::core::Result<HWND> {
    let instance = crate::com::instance();
    let class = WNDCLASSW {
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        lpszClassName: CLASS,
        ..Default::default()
    };
    // A second registration in the same process fails harmlessly.
    unsafe { RegisterClassW(&class) };
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            PCWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        )
    }
}

extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_MODES_WAITING {
        // A panic must not unwind into the application's message loop.
        if catch_unwind(AssertUnwindSafe(set_waiting_modes)).is_err() {
            tracing::warn!("setting a mode from outside panicked");
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
