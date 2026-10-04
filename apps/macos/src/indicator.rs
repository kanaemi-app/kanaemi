//! Shows the input mode near the caret for a moment after it changes.
//!
//! The IME draws this itself instead of declaring input modes to macOS, so
//! that every platform can show the same thing.

use std::cell::RefCell;

use kanaemi_core::Mode;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, msg_send, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSBox, NSBoxType, NSColor, NSFont, NSImage, NSImageView, NSPanel,
    NSTextAlignment, NSTextField, NSTitlePosition, NSWindowStyleMask,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

const VISIBLE_SECONDS: f64 = 0.8;
const SIZE: NSSize = NSSize::new(66.0, 26.0);
const ICON: NSSize = NSSize::new(16.0, 16.0);
// The logo's colours: the smile for かな, the keycap for ABC.
const KANA_COLOR: [u8; 3] = [0xD9, 0x50, 0x2E];
const ABC_COLOR: [u8; 3] = [0x1D, 0x24, 0x33];
// NSPopUpMenuWindowLevel: above ordinary windows, like a candidate panel.
const LEVEL: isize = 101;

thread_local! {
    static PANEL: RefCell<Option<Indicator>> = const { RefCell::new(None) };
}

struct Indicator {
    panel: Retained<NSPanel>,
    background: Retained<NSBox>,
    text: Retained<NSTextField>,
}

fn label(mode: Mode) -> &'static str {
    match mode {
        Mode::Kana => "かな",
        Mode::Abc => "ABC",
    }
}

fn color(mode: Mode) -> Retained<NSColor> {
    let [r, g, b] = match mode {
        Mode::Kana => KANA_COLOR,
        Mode::Abc => ABC_COLOR,
    };
    let channel = |c: u8| f64::from(c) / 255.0;
    NSColor::colorWithSRGBRed_green_blue_alpha(channel(r), channel(g), channel(b), 1.0)
}

/// The caret's line rectangle in screen coordinates, if the client reports one.
fn caret_rect(client: &AnyObject) -> Option<NSRect> {
    let mut rect = NSRect::ZERO;
    let _: Option<Retained<NSDictionary>> = unsafe {
        msg_send![client, attributesForCharacterIndex: 0usize, lineHeightRectangle: &mut rect]
    };
    (rect.size.height > 0.0).then_some(rect)
}

fn create(mtm: MainThreadMarker) -> Indicator {
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    let panel: Retained<NSPanel> = unsafe {
        msg_send![
            NSPanel::alloc(mtm),
            initWithContentRect: NSRect::new(NSPoint::ZERO, SIZE),
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    panel.setLevel(LEVEL);
    panel.setIgnoresMouseEvents(true);
    panel.setHasShadow(true);
    // Transparent, so that only the rounded box shows.
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    unsafe { panel.setReleasedWhenClosed(false) };

    let background = NSBox::initWithFrame(NSBox::alloc(mtm), NSRect::new(NSPoint::ZERO, SIZE));
    background.setBoxType(NSBoxType::Custom);
    background.setTitlePosition(NSTitlePosition::NoTitle);
    background.setBorderWidth(0.0);
    background.setCornerRadius(7.0);
    background.setContentViewMargins(NSSize::ZERO);

    let text = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    text.setFrame(NSRect::new(
        NSPoint::new(28.0, 4.0),
        NSSize::new(SIZE.width - 28.0 - 6.0, 17.0),
    ));
    text.setAlignment(NSTextAlignment::Center);
    text.setFont(Some(&NSFont::boldSystemFontOfSize(12.5)));
    text.setTextColor(Some(&NSColor::whiteColor()));

    if let Some(view) = panel.contentView() {
        view.addSubview(&background);
        // The logo's small symbol in white, its glyph cut out to show the colour.
        match NSImage::imageNamed(&NSString::from_str("indicator-icon")) {
            Some(image) => {
                let icon = NSImageView::imageViewWithImage(&image, mtm);
                icon.setFrame(NSRect::new(
                    NSPoint::new(8.0, (SIZE.height - ICON.height) / 2.0),
                    ICON,
                ));
                view.addSubview(&icon);
            }
            None => tracing::warn!("indicator icon missing from the bundle"),
        }
        view.addSubview(&text);
    }
    Indicator {
        panel,
        background,
        text,
    }
}

pub fn show(mode: Mode, client: &AnyObject) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let caret = caret_rect(client);
    tracing::debug!(?mode, ?caret, "mode indicator");
    let Some(caret) = caret else { return };
    PANEL.with_borrow_mut(|slot| {
        let Indicator {
            panel,
            background,
            text,
        } = slot.get_or_insert_with(|| create(mtm));
        background.setFillColor(&color(mode));
        text.setStringValue(&NSString::from_str(label(mode)));
        // AppKit's origin is bottom-left: place the label just below the line.
        let origin = NSPoint::new(caret.origin.x, caret.origin.y - SIZE.height - 4.0);
        panel.setFrame_display(NSRect::new(origin, SIZE), true);
        panel.orderFrontRegardless();
        unsafe {
            let _: () = msg_send![NSObject::class(), cancelPreviousPerformRequestsWithTarget: &**panel];
            let none: Option<&AnyObject> = None;
            let _: () = msg_send![&**panel, performSelector: sel!(orderOut:), withObject: none, afterDelay: VISIBLE_SECONDS];
        }
    });
}
