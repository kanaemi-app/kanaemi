//! Keys pressed and let go, as they happen. Input Method Kit passes on no
//! key's release; an event tap that only listens sees each one with its
//! time, which tells which of two keys let go close together went first.
//! Looking at the keyboard's state now and then cannot: a busy main thread
//! finds both up at once. The tap needs the Input Monitoring permission.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventMask, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType, CGPreflightListenEventAccess,
};

use crate::keys::POSTED_MARK;

/// Gets each key pressed or let go: its key code, whether it went down, and
/// its time on the clock of `NSEvent`'s timestamps.
pub type Seen = fn(u16, bool, u64);

thread_local! {
    static TAP: RefCell<Option<CFRetained<CFMachPort>>> = const { RefCell::new(None) };
    /// The tap was made but could not be set to run: not tried again.
    static BROKEN: Cell<bool> = const { Cell::new(false) };
    /// Whether a field of this IME has the focus, the only time the tap is on.
    static WANTED: Cell<bool> = const { Cell::new(false) };
}

pub fn running() -> bool {
    TAP.with_borrow(Option::is_some)
}

/// Starts the tap on the main run loop, unless it runs; returns whether it
/// runs. It cannot start without the permission.
pub fn start(seen: Seen) -> bool {
    if running() {
        return true;
    }
    // Asked on each field focused: cheaper than a tap made only to fail.
    if BROKEN.get() || !CGPreflightListenEventAccess() {
        return false;
    }
    let events: CGEventMask = (1 << CGEventType::KeyDown.0) | (1 << CGEventType::KeyUp.0);
    // SAFETY: `callback` matches CGEventTapCallBack, and `seen` is a plain
    // function pointer that outlives the tap.
    let tap = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            events,
            Some(callback),
            seen as *mut c_void,
        )
    };
    let Some(tap) = tap else {
        return false;
    };
    let (Some(source), Some(main)) = (
        CFMachPort::new_run_loop_source(None, Some(&tap), 0),
        CFRunLoop::main(),
    ) else {
        tracing::warn!("the event tap could not be run; keys let go are looked for instead");
        BROKEN.set(true);
        return false;
    };
    // SAFETY: a constant CoreFoundation exports.
    main.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    CGEvent::tap_enable(&tap, WANTED.get());
    TAP.set(Some(tap));
    true
}

/// Turns the tap on while a field of this IME has the focus, and off
/// otherwise, so keys typed elsewhere are not listened to.
pub fn want(on: bool) {
    WANTED.set(on);
    TAP.with_borrow(|tap| {
        if let Some(tap) = tap {
            CGEvent::tap_enable(tap, on);
        }
    });
}

/// The time of a tap's event on the clock of `NSEvent`'s timestamps, the
/// time since startup. The event's own time is in the ticks of
/// `mach_absolute_time` or in nanoseconds, by the macOS release; ticks and
/// nanoseconds differ many times over on Apple silicon, so the reading that
/// lands nearer now is the one.
fn event_ms(timestamp: u64) -> u64 {
    let (numer, denom) = timebase();
    let ms =
        |ticks: u64| (u128::from(ticks) * u128::from(numer) / u128::from(denom) / 1_000_000) as u64;
    // SAFETY: no arguments, no conditions.
    let now = ms(unsafe { mach_absolute_time() });
    let as_ticks = ms(timestamp);
    let as_nanos = timestamp / 1_000_000;
    if as_ticks.abs_diff(now) <= as_nanos.abs_diff(now) {
        as_ticks
    } else {
        as_nanos
    }
}

#[repr(C)]
#[derive(Default)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

unsafe extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
}

fn timebase() -> (u32, u32) {
    thread_local! {
        static TIMEBASE: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
    }
    TIMEBASE.get().unwrap_or_else(|| {
        let mut info = MachTimebaseInfo::default();
        // SAFETY: `info` is a valid place to write to.
        let found = unsafe { mach_timebase_info(&mut info) } == 0 && info.denom != 0;
        let timebase = if found {
            (info.numer, info.denom)
        } else {
            (1, 1)
        };
        TIMEBASE.set(Some(timebase));
        timebase
    })
}

unsafe extern "C-unwind" fn callback(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    info: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: the tap passes an event it owns for the call.
    let cg = unsafe { event.as_ref() };
    match kind {
        // macOS turns a tap off that it finds slow; it is turned on again
        // while it is wanted. What it missed is found by the keyboard's state.
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
            want(WANTED.get());
        }
        CGEventType::KeyDown | CGEventType::KeyUp
            // A key the IME posted never reaches it as typed.
            if CGEvent::integer_value_field(Some(cg), CGEventField::EventSourceUserData)
                != POSTED_MARK =>
        {
            let code =
                CGEvent::integer_value_field(Some(cg), CGEventField::KeyboardEventKeycode) as u16;
            // SAFETY: `info` is the `Seen` given to `start`.
            let seen: Seen = unsafe { std::mem::transmute::<*mut c_void, Seen>(info) };
            seen(code, kind == CGEventType::KeyDown, event_ms(CGEvent::timestamp(Some(cg))));
        }
        _ => {}
    }
    event.as_ptr()
}
