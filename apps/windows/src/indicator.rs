//! Shows the input mode near the caret for a moment after it changes:
//! 「かな」 or 「ABC」 in white on the logo's colour for the mode.
//!
//! The IME draws this itself instead of declaring input modes to Windows, so
//! that every platform can show the same thing.

use std::cell::Cell;

use kanaemi_core::Mode;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::popup::{self, Popup};

const CLASS: PCWSTR = w!("KanaemiModeIndicator");
const VISIBLE_MS: u32 = 800;
const TIMER: usize = 1;
const PADDING: i32 = 8;
// The logo's colours, as 0x00BBGGRR: the smile for かな, the keycap for ABC.
const KANA_COLOR: COLORREF = COLORREF(0x002E_50D9);
const ABC_COLOR: COLORREF = COLORREF(0x0033_241D);

thread_local! {
    static POPUP: Popup = const { Popup::new() };
    static SHOWN: Cell<Mode> = const { Cell::new(Mode::Abc) };
}

fn label(mode: Mode) -> &'static str {
    match mode {
        Mode::Kana => "かな",
        Mode::Abc => "ABC",
    }
}

/// Shows `mode` below `at`, a screen rectangle of the caret, until a moment
/// passes.
pub fn show(mode: Mode, at: RECT, owner: Option<HWND>) {
    let Some(window) = POPUP.with(|popup| popup.owned_by(owner, create)) else {
        return;
    };
    SHOWN.set(mode);
    let (width, height) = size(window, mode);
    let (left, top) = popup::position(at, (width, height), 2);
    unsafe {
        let _ = SetWindowPos(
            window,
            Some(HWND_TOPMOST),
            left,
            top,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = InvalidateRect(Some(window), None, true);
        SetTimer(Some(window), TIMER, VISIBLE_MS, None);
    }
}

pub fn hide() {
    if let Some(window) = POPUP.with(Popup::existing) {
        unsafe {
            let _ = KillTimer(Some(window), TIMER);
            let _ = ShowWindow(window, SW_HIDE);
        }
    }
}

fn create(owner: Option<HWND>) -> Result<HWND> {
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
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            CLASS,
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            owner,
            None,
            Some(instance),
            None,
        )
    }
}

fn size(window: HWND, mode: Mode) -> (i32, i32) {
    let dc = unsafe { GetDC(Some(window)) };
    let previous = unsafe { SelectObject(dc, crate::candidates::font().into()) };
    let text: Vec<u16> = label(mode).encode_utf16().collect();
    let mut size = SIZE::default();
    unsafe {
        let _ = GetTextExtentPoint32W(dc, &text, &mut size);
        SelectObject(dc, previous);
        ReleaseDC(Some(window), dc);
    }
    (size.cx + PADDING * 2, size.cy + PADDING)
}

fn paint(window: HWND) {
    let mode = SHOWN.get();
    let mut ps = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(window, &mut ps) };
    let previous = unsafe { SelectObject(dc, crate::candidates::font().into()) };
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(window, &mut client);
        let brush = CreateSolidBrush(match mode {
            Mode::Kana => KANA_COLOR,
            Mode::Abc => ABC_COLOR,
        });
        FillRect(dc, &client, brush);
        let _ = DeleteObject(brush.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00ff_ffff));
        let mut text: Vec<u16> = label(mode).encode_utf16().collect();
        DrawTextW(
            dc,
            &mut text,
            &mut client,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dc, previous);
        let _ = EndPaint(window, &ps);
    }
}

extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            // A panic must not unwind into the application's message loop.
            let _ = std::panic::catch_unwind(|| paint(window));
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER => {
            hide();
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}
