//! The candidate window: a popup beside the composition listing the page of
//! candidates, numbered from 1, with the selected one highlighted. Beside
//! each candidate, smaller and greyed, is the dictionary it came from or,
//! for a reading to complete with, its first candidate; a highlighted
//! reading's other candidates fill a pane right of the list. It never takes
//! the focus from the application.

use std::cell::RefCell;

use crate::listing::{self, Layout, PADDING, Page, Widths};
use crate::popup::{self, Popup};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

const CLASS: PCWSTR = w!("KanaemiCandidates");

/// What a click on a candidate calls, with its position on the page.
type Picked = Box<dyn Fn(usize)>;

thread_local! {
    static POPUP: Popup = const { Popup::new() };
    static SHOWN: RefCell<Page> = RefCell::new(Page::default());
    static FONT: RefCell<Option<HFONT>> = const { RefCell::new(None) };
    static SMALL_FONT: RefCell<Option<HFONT>> = const { RefCell::new(None) };
    static PICKED: RefCell<Option<Picked>> = const { RefCell::new(None) };
}

/// Calls `picked` with the position on the page of a candidate clicked in
/// the window of this thread.
pub fn on_pick(picked: impl Fn(usize) + 'static) {
    PICKED.set(Some(Box::new(picked)));
}

/// Shows `page` below `at`, a screen rectangle of the composition, in a
/// window `owner` owns, the application's.
pub fn show(page: Page, at: RECT, owner: Option<HWND>) {
    let Some(window) = POPUP.with(|popup| popup.owned_by(owner, create)) else {
        return;
    };
    SHOWN.set(page);
    let laid = measure(window);
    let border = 2;
    let (width, height) = (laid.width + border, laid.height + border);
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
fn message_font() -> Option<LOGFONTW> {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            metrics.cbSize,
            Some((&mut metrics as *mut NONCLIENTMETRICSW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()?;
    Some(metrics.lfMessageFont)
}

/// The font candidates are drawn in.
pub(crate) fn font() -> HFONT {
    FONT.with_borrow_mut(|font| {
        *font.get_or_insert_with(|| match message_font() {
            Some(logfont) => unsafe { CreateFontIndirectW(&logfont) },
            None => HFONT(unsafe { GetStockObject(DEFAULT_GUI_FONT) }.0),
        })
    })
}

/// The smaller font of what is beside a candidate.
fn small_font() -> HFONT {
    SMALL_FONT.with_borrow_mut(|font| {
        *font.get_or_insert_with(|| match message_font() {
            Some(mut logfont) => {
                logfont.lfHeight = logfont.lfHeight * 5 / 6;
                unsafe { CreateFontIndirectW(&logfont) }
            }
            None => HFONT(unsafe { GetStockObject(DEFAULT_GUI_FONT) }.0),
        })
    })
}

fn extent(dc: HDC, font: HFONT, text: &str) -> SIZE {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    unsafe {
        let previous = SelectObject(dc, font.into());
        let _ = GetTextExtentPoint32W(dc, &wide, &mut size);
        SelectObject(dc, previous);
    }
    size
}

/// The height of a row.
fn line(dc: HDC) -> i32 {
    extent(dc, font(), "あ").cy + PADDING / 2
}

/// Lays out the page shown, measured in `window`.
fn measure(window: HWND) -> Layout {
    let dc = unsafe { GetDC(Some(window)) };
    let laid = SHOWN.with_borrow(|page| {
        let widths: Vec<Widths> = page
            .items
            .iter()
            .zip(&page.beside)
            .enumerate()
            .map(|(i, (item, beside))| Widths {
                number: extent(dc, font(), &(i + 1).to_string()).cx,
                surface: extent(dc, font(), item).cx,
                beside: beside
                    .as_deref()
                    .map_or(0, |b| extent(dc, small_font(), b).cx),
            })
            .collect();
        let pane = (!page.more.is_empty()).then(|| {
            let widest = page
                .more
                .iter()
                .map(|m| extent(dc, font(), m).cx)
                .max()
                .unwrap_or(0);
            (page.more.len(), widest)
        });
        let footer = page
            .footer
            .as_deref()
            .map(|footer| extent(dc, small_font(), footer).cx);
        listing::layout(&widths, line(dc), pane, footer)
    });
    unsafe { ReleaseDC(Some(window), dc) };
    laid
}

/// Draws `text` within `width` from `(x, top)`, its end cut with an ellipsis
/// when it is wider.
fn text_out(dc: HDC, text: &str, x: i32, top: i32, width: i32, height: i32) {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let mut area = RECT {
        left: x,
        top,
        right: x + width,
        bottom: top + height,
    };
    unsafe {
        DrawTextW(
            dc,
            &mut wide,
            &mut area,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }
}

fn paint(window: HWND) {
    let laid = measure(window);
    let mut ps = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(window, &mut ps) };
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(window, &mut client);
        FillRect(dc, &client, GetSysColorBrush(COLOR_WINDOW));
        SetBkMode(dc, TRANSPARENT);
    }
    let line = line(dc);
    SHOWN.with_borrow(|page| {
        if let Some((x, width)) = laid.pane {
            let pane = RECT {
                left: x,
                top: 0,
                right: x + width,
                bottom: client.bottom,
            };
            unsafe {
                FillRect(dc, &pane, GetSysColorBrush(COLOR_BTNFACE));
                SetTextColor(dc, COLORREF(GetSysColor(COLOR_WINDOWTEXT)));
                SelectObject(dc, font().into());
            }
            for (i, other) in page.more.iter().enumerate() {
                let top = PADDING / 2 + line * i as i32;
                text_out(dc, other, x + PADDING, top, width - PADDING * 2, line);
            }
        }
        for (i, (item, beside)) in page.items.iter().zip(&page.beside).enumerate() {
            let top = PADDING / 2 + line * i as i32;
            let row = RECT {
                left: 0,
                top,
                right: laid.list_width,
                bottom: top + line,
            };
            let selected = i == page.selected;
            let (background, foreground, muted) = if selected {
                (COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_HIGHLIGHTTEXT)
            } else {
                (COLOR_WINDOW, COLOR_WINDOWTEXT, COLOR_GRAYTEXT)
            };
            unsafe {
                FillRect(dc, &row, GetSysColorBrush(background));
                SetTextColor(dc, COLORREF(GetSysColor(foreground)));
                SelectObject(dc, font().into());
            }
            let number = (i + 1).to_string();
            text_out(
                dc,
                &number,
                laid.number_x,
                top,
                laid.surface_x - laid.number_x,
                line,
            );
            text_out(dc, item, laid.surface_x, top, laid.surface_width, line);
            if let Some(beside) = beside {
                unsafe {
                    SetTextColor(dc, COLORREF(GetSysColor(muted)));
                    SelectObject(dc, small_font().into());
                }
                text_out(dc, beside, laid.beside_x, top, laid.beside_width, line);
            }
        }
        if let (Some((x, top)), Some(footer)) = (laid.footer, &page.footer) {
            unsafe {
                SetTextColor(dc, COLORREF(GetSysColor(COLOR_GRAYTEXT)));
                SelectObject(dc, small_font().into());
            }
            text_out(dc, footer, x, top, laid.list_width - x, line);
        }
    });
    unsafe {
        SelectObject(dc, font().into());
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
            let x = i32::from(lparam.0 as i16);
            let y = i32::from((lparam.0 >> 16) as i16);
            let _ = std::panic::catch_unwind(|| {
                let laid = measure(window);
                let dc = unsafe { GetDC(Some(window)) };
                let line = line(dc);
                unsafe { ReleaseDC(Some(window), dc) };
                let rows = SHOWN.with_borrow(|page| page.items.len());
                if let Some(row) = listing::row_at(x, y, rows, line, laid.list_width) {
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
