//! A popup window of the text service, owned by the application's window.
//!
//! A window shows in the band of the window that owns it when it is made:
//! the Start menu and other immersive windows sit in a band above every
//! topmost window, so a popup made without an owner, or given one later,
//! stays hidden beneath them. The popup is made again whenever its owner
//! changes.

use std::cell::Cell;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, IsWindow};
use windows::core::Result;

use crate::placement::{Rect, place};

#[derive(Default)]
pub(crate) struct Popup {
    window: Cell<Option<HWND>>,
    owner: Cell<Option<HWND>>,
}

impl Popup {
    pub(crate) const fn new() -> Self {
        Self {
            window: Cell::new(None),
            owner: Cell::new(None),
        }
    }

    /// The popup owned by `owner`, made by `make` when there is none yet or
    /// it belongs to another owner.
    pub(crate) fn owned_by(
        &self,
        owner: Option<HWND>,
        make: impl FnOnce(Option<HWND>) -> Result<HWND>,
    ) -> Option<HWND> {
        if let Some(window) = self.existing() {
            if self.owner.get() == owner {
                return Some(window);
            }
            unsafe {
                let _ = DestroyWindow(window);
            }
            self.window.set(None);
        }
        let window = make(owner)
            .inspect_err(|error| tracing::warn!(%error, "popup window not created"))
            .ok()?;
        self.window.set(Some(window));
        self.owner.set(owner);
        Some(window)
    }

    /// The popup as it is, if it was made and is still there: closing its
    /// owner destroys it too.
    pub(crate) fn existing(&self) -> Option<HWND> {
        let window = self
            .window
            .get()
            .filter(|w| unsafe { IsWindow(Some(*w)) }.as_bool());
        if window.is_none() {
            self.window.set(None);
        }
        window
    }
}

/// Where a popup of `size` goes by `at`, a screen rectangle of the text,
/// kept on the monitor that shows the text.
pub(crate) fn position(at: RECT, size: (i32, i32), gap: i32) -> (i32, i32) {
    let rect = |r: RECT| Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    };
    let monitor = unsafe { MonitorFromRect(&at, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let work = if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        rect(info.rcWork)
    } else {
        // No monitor to ask: leave the popup where the text is.
        Rect {
            left: i32::MIN / 2,
            top: i32::MIN / 2,
            right: i32::MAX / 2,
            bottom: i32::MAX / 2,
        }
    };
    place(rect(at), size, gap, work)
}
