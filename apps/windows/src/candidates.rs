//! The candidate window: a popup beside the composition listing the page of
//! candidates, numbered from 1, with the selected one highlighted. It never
//! takes the focus from the application.

use std::cell::RefCell;

use crate::popup::{self, Popup};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

const CLASS: PCWSTR = w!("KanaemiCandidates");
/// Space around the rows and between a number and its candidate, in pixels.
const PADDING: i32 = 6;

/// What a click on a candidate calls, with its position on the page.
type Picked = Box<dyn Fn(usize)>;

#[derive(Default)]
struct Shown {
    items: Vec<String>,
    selected: usize,
}

thread_local! {
    static POPUP: Popup = const { Popup::new() };
    static SHOWN: RefCell<Shown> = RefCell::new(Shown::default());
    static FONT: RefCell<Option<HFONT>> = const { RefCell::new(None) };
    static PICKED: RefCell<Option<Picked>> = const { RefCell::new(None) };
}

/// Calls `picked` with the position on the page of a candidate clicked in
/// the window of this thread.
pub fn on_pick(picked: impl Fn(usize) + 'static) {
    PICKED.set(Some(Box::new(picked)));
}

/// Shows `items` below `at`, a screen rectangle of the composition, in a
/// window `owner` owns, the application's.
pub fn show(items: Vec<String>, selected: usize, at: RECT, owner: Option<HWND>) {
    let Some(window) = POPUP.with(|popup| popup.owned_by(owner, create)) else {
        return;
    };
    SHOWN.set(Shown { items, selected });
    let (width, height) = size(window);
    let (left, top) = popup::position(at, (width, height), 0);
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
    }
}

pub fn hide() {
    if let Some(window) = POPUP.with(Popup::existing) {
        unsafe {
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
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        ..Default::default()
    };
    // A second registration in the same process fails harmlessly.
    unsafe { RegisterClassW(&class) };
    unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            CLASS,
            PCWSTR::null(),
            WS_POPUP | WS_BORDER,
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

/// The system's message font, which covers Japanese.
pub(crate) fn font() -> HFONT {
    FONT.with_borrow_mut(|font| {
        *font.get_or_insert_with(|| {
            let mut metrics = NONCLIENTMETRICSW {
                cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
                ..Default::default()
            };
            let found = unsafe {
                SystemParametersInfoW(
                    SPI_GETNONCLIENTMETRICS,
                    metrics.cbSize,
                    Some((&mut metrics as *mut NONCLIENTMETRICSW).cast()),
                    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                )
            };
            if found.is_err() {
                return HFONT(unsafe { GetStockObject(DEFAULT_GUI_FONT) }.0);
            }
            unsafe { CreateFontIndirectW(&metrics.lfMessageFont) }
        })
    })
}

fn rows() -> Vec<(String, String)> {
    SHOWN.with_borrow(|shown| {
        shown
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| (format!("{}", i + 1), item.clone()))
            .collect()
    })
}

/// The height of a row.
fn line(dc: HDC) -> i32 {
    extent(dc, "あ").cy + PADDING / 2
}

/// The row at `y` in the window, if a candidate is shown there.
fn row_at(window: HWND, y: i32) -> Option<usize> {
    let dc = unsafe { GetDC(Some(window)) };
    let previous = unsafe { SelectObject(dc, font().into()) };
    let line = line(dc);
    unsafe {
        SelectObject(dc, previous);
        ReleaseDC(Some(window), dc);
    }
    let row = usize::try_from((y - PADDING / 2).div_euclid(line)).ok()?;
    (row < SHOWN.with_borrow(|shown| shown.items.len())).then_some(row)
}

fn extent(dc: HDC, text: &str) -> SIZE {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    unsafe {
        let _ = GetTextExtentPoint32W(dc, &wide, &mut size);
    }
    size
}

/// The window's size for the rows shown, with the number column first.
fn size(window: HWND) -> (i32, i32) {
    let dc = unsafe { GetDC(Some(window)) };
    let previous = unsafe { SelectObject(dc, font().into()) };
    let rows = rows();
    let number = rows
        .iter()
        .map(|(n, _)| extent(dc, n).cx)
        .max()
        .unwrap_or(0);
    let text = rows
        .iter()
        .map(|(_, t)| extent(dc, t).cx)
        .max()
        .unwrap_or(0);
    let line = line(dc);
    unsafe {
        SelectObject(dc, previous);
        ReleaseDC(Some(window), dc);
    }
    let border = 2;
    (
        number + text + PADDING * 3 + border,
        line * rows.len() as i32 + PADDING + border,
    )
}

fn paint(window: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(window, &mut ps) };
    let previous = unsafe { SelectObject(dc, font().into()) };
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(window, &mut client);
        FillRect(dc, &client, GetSysColorBrush(COLOR_WINDOW));
        SetBkMode(dc, TRANSPARENT);
    }
    let rows = rows();
    let selected = SHOWN.with_borrow(|shown| shown.selected);
    let number_width = rows
        .iter()
        .map(|(n, _)| extent(dc, n).cx)
        .max()
        .unwrap_or(0);
    let line = line(dc);
    for (i, (number, text)) in rows.iter().enumerate() {
        let top = PADDING / 2 + line * i as i32;
        let row = RECT {
            left: 0,
            top,
            right: client.right,
            bottom: top + line,
        };
        let (background, foreground) = if i == selected {
            (COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT)
        } else {
            (COLOR_WINDOW, COLOR_WINDOWTEXT)
        };
        unsafe {
            FillRect(dc, &row, GetSysColorBrush(background));
            SetTextColor(dc, COLORREF(GetSysColor(foreground)));
        }
        for (x, text) in [(PADDING, number), (PADDING * 2 + number_width, text)] {
            let wide: Vec<u16> = text.encode_utf16().collect();
            unsafe {
                let _ = TextOutW(dc, x, top + PADDING / 4, &wide);
            }
        }
    }
    unsafe {
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
        WM_LBUTTONUP => {
            let y = i32::from((lparam.0 >> 16) as i16);
            let _ = std::panic::catch_unwind(|| {
                if let Some(row) = row_at(window, y) {
                    PICKED.with_borrow(|picked| picked.as_ref().map(|picked| picked(row)));
                }
            });
            LRESULT(0)
        }
        // Clicking the window must not take the focus from the field.
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}
