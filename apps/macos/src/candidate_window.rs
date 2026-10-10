//! The candidate window Kanaemi draws itself, in place of Input Method
//! Kit's panel, which shows plain strings only: each candidate with the
//! dictionary it came from, and beside the list, the highlighted one's
//! meaning from the dictionaries of macOS once it is looked up. It never
//! takes the focus from the application.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};

use kanaemi_core::CandidateView;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSAccessibilityAnnouncementKey, NSAccessibilityAnnouncementRequestedNotification,
    NSAccessibilityPostNotificationWithUserInfo, NSAccessibilityPriorityKey,
    NSAccessibilityPriorityLevel, NSBackingStoreType, NSBezierPath, NSColor,
    NSCompositingOperation, NSEvent, NSFont, NSFontAttributeName, NSFontWeightRegular,
    NSForegroundColorAttributeName, NSImage, NSImageSymbolConfiguration, NSLineBreakMode,
    NSMutableParagraphStyle, NSPanel, NSParagraphStyleAttributeName, NSResponder, NSScreen,
    NSStringDrawing, NSStringDrawingOptions, NSStringNSExtendedStringDrawing, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{
    NSDictionary, NSNotFound, NSNumber, NSObject, NSPoint, NSRange, NSRect, NSSize, NSString,
};

use crate::candidates::{
    self, Layout, MARK_WIDTH, Meanings, PADDING, Rect, Row, RowWidths, Size, footer, layout,
    meaning_wanted, pane_text, place, row_at, spoken, to_look_up, window_level,
};
use crate::meaning;

const CANDIDATE_SIZE: f64 = 16.0;
const NUMBER_SIZE: f64 = 12.0;
const SOURCE_SIZE: f64 = 11.0;
const MEANING_SIZE: f64 = 12.0;
/// Space above and below a candidate within its row.
const ROW_SPACE: f64 = 6.0;
/// The window's corner radius: modest, and within the padding, so the first
/// and last rows are never clipped by the rounding.
const CORNER: f64 = 4.0;
const _: () = assert!(CORNER < PADDING);
/// The widest a meaning is set, and the tallest; a longer one is cut.
const MEANING_WIDTH: f64 = 260.0;
const MEANING_HEIGHT: f64 = 200.0;

/// What a click on a candidate calls, with its position on the page.
type Picked = fn(usize);

thread_local! {
    static WINDOW: RefCell<Option<Window>> = const { RefCell::new(None) };
    static PICKED: Cell<Option<Picked>> = const { Cell::new(None) };
    static MEANINGS: RefCell<Meanings> = RefCell::new(Meanings::default());
}

struct Window {
    panel: Retained<NSPanel>,
    list: Retained<ListView>,
    /// The caret's line the window was placed by, kept while the page stays.
    caret: Rect,
}

/// The text attributes each part is drawn with. The colours are the
/// system's, resolved as they are drawn, so they follow light and dark.
struct Style {
    number: Retained<NSDictionary<NSString, AnyObject>>,
    surface: Retained<NSDictionary<NSString, AnyObject>>,
    source: Retained<NSDictionary<NSString, AnyObject>>,
    highlighted_number: Retained<NSDictionary<NSString, AnyObject>>,
    highlighted_surface: Retained<NSDictionary<NSString, AnyObject>>,
    highlighted_source: Retained<NSDictionary<NSString, AnyObject>>,
    meaning: Retained<NSDictionary<NSString, AnyObject>>,
    /// What marks a candidate with a meaning; `None` when the system has no
    /// such symbol.
    mark: Option<Retained<NSImage>>,
    highlighted_mark: Option<Retained<NSImage>>,
}

/// A book, tinted `color`: there is more to read of the candidate.
fn mark(color: &NSColor) -> Option<Retained<NSImage>> {
    let book = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str("book.closed"),
        Some(&NSString::from_str("意味")),
    )?;
    let size = NSImageSymbolConfiguration::configurationWithPointSize_weight(SOURCE_SIZE, unsafe {
        NSFontWeightRegular
    });
    let tint = NSImageSymbolConfiguration::configurationWithHierarchicalColor(color);
    book.imageWithSymbolConfiguration(&size.configurationByApplyingConfiguration(&tint))
}

/// Text that wraps, as a meaning does.
fn attributes(font: &NSFont, color: &NSColor) -> Retained<NSDictionary<NSString, AnyObject>> {
    let font: Retained<AnyObject> = Retained::into_super(Retained::into_super(font.retain()));
    let color: Retained<AnyObject> = Retained::into_super(Retained::into_super(color.retain()));
    // SAFETY: the keys are AppKit's, which live as long as it.
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    NSDictionary::from_retained_objects(&keys, &[font, color])
}

/// Text on one line, its end cut with an ellipsis when it is wider than
/// where it is drawn, as a row's parts are.
fn one_line(font: &NSFont, color: &NSColor) -> Retained<NSDictionary<NSString, AnyObject>> {
    let paragraph = NSMutableParagraphStyle::new();
    paragraph.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    let font: Retained<AnyObject> = Retained::into_super(Retained::into_super(font.retain()));
    let color: Retained<AnyObject> = Retained::into_super(Retained::into_super(color.retain()));
    let paragraph: Retained<AnyObject> =
        Retained::into_super(Retained::into_super(Retained::into_super(paragraph)));
    // SAFETY: the keys are AppKit's, which live as long as it.
    let keys = unsafe {
        [
            NSFontAttributeName,
            NSForegroundColorAttributeName,
            NSParagraphStyleAttributeName,
        ]
    };
    NSDictionary::from_retained_objects(&keys, &[font, color, paragraph])
}

impl Style {
    fn new() -> Self {
        let candidate = NSFont::systemFontOfSize(CANDIDATE_SIZE);
        let number = NSFont::systemFontOfSize(NUMBER_SIZE);
        let source = NSFont::systemFontOfSize(SOURCE_SIZE);
        let meaning = NSFont::systemFontOfSize(MEANING_SIZE);
        let on_highlight = NSColor::alternateSelectedControlTextColor();
        let muted_on_highlight = on_highlight.colorWithAlphaComponent(0.75);
        Self {
            number: one_line(&number, &NSColor::secondaryLabelColor()),
            surface: one_line(&candidate, &NSColor::labelColor()),
            source: one_line(&source, &NSColor::secondaryLabelColor()),
            highlighted_number: one_line(&number, &muted_on_highlight),
            highlighted_surface: one_line(&candidate, &on_highlight),
            highlighted_source: one_line(&source, &muted_on_highlight),
            meaning: attributes(&meaning, &NSColor::secondaryLabelColor()),
            mark: mark(&NSColor::secondaryLabelColor()),
            highlighted_mark: mark(&muted_on_highlight),
        }
    }
}

thread_local! {
    static STYLE: Style = Style::new();
}

/// What the list view draws.
#[derive(Default)]
struct Content {
    rows: Vec<Row>,
    selected: usize,
    /// Beside `rows`: whether the candidate is known to have a meaning.
    marks: Vec<bool>,
    /// The highlighted candidate whose meaning is shown, when it has one.
    highlighted: Option<String>,
    /// For a reading to complete with, its candidates after the first.
    more: Vec<String>,
    meaning: Option<String>,
    /// Which page is shown of how many, when there are more than one.
    footer: Option<String>,
    row_height: f64,
    layout: Layout,
}

define_class!(
    // SAFETY: NSView is designed to be subclassed; no Drop is added.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KanaemiCandidateList"]
    #[ivars = RefCell<Content>]
    struct ListView;

    impl ListView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            guarded(|| self.draw());
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            guarded(|| self.click(event));
        }
    }
);

impl ListView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RefCell::new(Content::default()));
        unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] }
    }

    fn draw(&self) {
        let content = self.ivars().borrow();
        let laid = content.layout;
        let bounds = self.bounds();
        let outline =
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, CORNER, CORNER);
        NSColor::windowBackgroundColor().setFill();
        outline.fill();
        outline.addClip();
        STYLE.with(|style| {
            if let (Some(pane), Some(meaning)) = (laid.pane, &content.meaning) {
                NSColor::quaternaryLabelColor().setFill();
                NSBezierPath::fillRect(ns_rect(pane));
                let text = NSString::from_str(meaning);
                let area = NSRect::new(
                    NSPoint::new(pane.x + PADDING, PADDING),
                    NSSize::new(pane.width - PADDING * 2.0, pane.height - PADDING * 2.0),
                );
                unsafe {
                    text.drawWithRect_options_attributes_context(
                        area,
                        meaning_options() | NSStringDrawingOptions::TruncatesLastVisibleLine,
                        Some(&style.meaning),
                        None,
                    );
                }
            }
            for (index, row) in content.rows.iter().enumerate() {
                let top = PADDING + content.row_height * index as f64;
                let highlighted = index == content.selected;
                if highlighted {
                    let band = NSRect::new(
                        NSPoint::new(PADDING / 2.0, top),
                        NSSize::new(laid.list_width - PADDING, content.row_height),
                    );
                    NSColor::selectedContentBackgroundColor().setFill();
                    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(band, 4.0, 4.0).fill();
                }
                let (number, surface, source, mark) = if highlighted {
                    (
                        &style.highlighted_number,
                        &style.highlighted_surface,
                        &style.highlighted_source,
                        &style.highlighted_mark,
                    )
                } else {
                    (&style.number, &style.surface, &style.source, &style.mark)
                };
                if content.marks.get(index).copied().unwrap_or(false)
                    && let Some(mark) = mark
                {
                    let size = mark.size();
                    let scale = (MARK_WIDTH / size.width).min(1.0);
                    let (width, height) = (size.width * scale, size.height * scale);
                    let area = NSRect::new(
                        NSPoint::new(
                            laid.mark_x + (MARK_WIDTH - width) / 2.0,
                            top + (content.row_height - height) / 2.0,
                        ),
                        NSSize::new(width, height),
                    );
                    unsafe {
                        mark.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                            area,
                            NSRect::ZERO,
                            NSCompositingOperation::SourceOver,
                            1.0,
                            true,
                            None,
                        );
                    }
                }
                let parts = [
                    (
                        laid.number_x,
                        laid.surface_x - laid.number_x,
                        Some(row.number.as_str()),
                        number,
                    ),
                    (
                        laid.surface_x,
                        laid.surface_width,
                        Some(row.surface.as_str()),
                        surface,
                    ),
                    (laid.source_x, laid.source_width, row.beside(), source),
                ];
                for (x, width, text, attributes) in parts {
                    let Some(text) = text else { continue };
                    let text = NSString::from_str(text);
                    // Each part sits on the middle of the row, whatever its size.
                    let height = unsafe { text.sizeWithAttributes(Some(attributes)) }.height;
                    let y = top + (content.row_height - height) / 2.0;
                    let area = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));
                    unsafe { text.drawInRect_withAttributes(area, Some(attributes)) };
                }
            }
            if let (Some(area), Some(footer)) = (laid.footer, &content.footer) {
                let text = NSString::from_str(footer);
                unsafe { text.drawInRect_withAttributes(ns_rect(area), Some(&style.source)) };
            }
        });
        NSColor::separatorColor().setStroke();
        outline.setLineWidth(1.0);
        outline.stroke();
    }

    fn click(&self, event: &NSEvent) {
        let at = self.convertPoint_fromView(event.locationInWindow(), None);
        let row = {
            let content = self.ivars().borrow();
            row_at(
                at.x,
                at.y,
                content.rows.len(),
                content.row_height,
                content.layout.list_width,
            )
        };
        if let (Some(row), Some(picked)) = (row, PICKED.get()) {
            picked(row);
        }
    }
}

fn meaning_options() -> NSStringDrawingOptions {
    NSStringDrawingOptions::UsesLineFragmentOrigin | NSStringDrawingOptions::UsesFontLeading
}

/// Runs an AppKit callback: a panic must not unwind into AppKit, which would
/// end the IME.
fn guarded(callback: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(callback)).is_err() {
        tracing::warn!("drawing the candidate window panicked");
    }
}

fn ns_rect(rect: Rect) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.x, rect.y),
        NSSize::new(rect.width, rect.height),
    )
}

fn rect(rect: NSRect) -> Rect {
    Rect {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

fn create(mtm: MainThreadMarker) -> Window {
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    let panel: Retained<NSPanel> = unsafe {
        msg_send![
            NSPanel::alloc(mtm),
            initWithContentRect: NSRect::ZERO,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: true
        ]
    };
    panel.setLevel(window_level(0));
    // Shown over whichever Space has the field, a full-screen one included.
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    panel.setHasShadow(true);
    // Transparent, so that only the rounded list shows.
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHidesOnDeactivate(false);
    panel.setBecomesKeyOnlyIfNeeded(true);
    unsafe { panel.setReleasedWhenClosed(false) };
    let list = ListView::new(mtm);
    panel.setContentView(Some(&list));
    Window {
        panel,
        list,
        caret: Rect::default(),
    }
}

/// Has a screen reader say `text`, as it cannot read what the window draws;
/// nothing happens without one.
fn announce(list: &ListView, text: &str) {
    let text: Retained<AnyObject> =
        Retained::into_super(Retained::into_super(NSString::from_str(text)));
    let priority: Retained<AnyObject> = Retained::into_super(Retained::into_super(
        Retained::into_super(NSNumber::new_isize(NSAccessibilityPriorityLevel::High.0)),
    ));
    // SAFETY: AppKit's keys and notification name, with the values the
    // announcement takes: an NSString and an NSNumber.
    unsafe {
        let info = NSDictionary::from_retained_objects(
            &[NSAccessibilityAnnouncementKey, NSAccessibilityPriorityKey],
            &[text, priority],
        );
        NSAccessibilityPostNotificationWithUserInfo(
            list,
            NSAccessibilityAnnouncementRequestedNotification,
            Some(&info),
        );
    }
}

/// Calls `picked` with the position on the page of a candidate clicked.
pub fn on_pick(picked: Picked) {
    PICKED.set(Some(picked));
}

/// Shows the page `view` by the caret of `client`, or brings the window up
/// to date with it.
pub fn show(view: &CandidateView, client: &AnyObject) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let rows = candidates::rows(view);
    // Every candidate on the page is looked up, to mark those with a
    // meaning; the answers come back to `meaning_found`.
    if let Some(page) = MEANINGS.with_borrow_mut(|m| m.want(to_look_up(view))) {
        meaning::ask(page);
    }
    let highlighted = meaning_wanted(view).map(str::to_owned);
    WINDOW.with_borrow_mut(|slot| {
        let window = slot.get_or_insert_with(|| create(mtm));
        let page_changed = window.list.ivars().borrow().rows != rows;
        let moved = page_changed
            || !window.panel.isVisible()
            || window.list.ivars().borrow().selected != view.selected;
        if moved && let Some(row) = rows.get(view.selected) {
            announce(&window.list, &spoken(row));
        }
        if page_changed || !window.panel.isVisible() {
            // The client's own window may float above menus. Its level is a
            // CGWindowLevel, 32 bits wide.
            let client_level: i32 = unsafe { msg_send![client, windowLevel] };
            window.panel.setLevel(window_level(client_level as isize));
            window.caret = caret_rect(client).unwrap_or_else(|| {
                tracing::debug!("no caret to place the candidates by; placed by the pointer");
                let pointer = NSEvent::mouseLocation();
                Rect {
                    x: pointer.x,
                    y: pointer.y,
                    width: 0.0,
                    height: 0.0,
                }
            });
        }
        {
            let mut content = window.list.ivars().borrow_mut();
            content.rows = rows;
            content.selected = view.selected;
            content.highlighted = highlighted;
            content.more = view.more.clone();
            content.footer = footer(view);
            mark_meanings(&mut content);
        }
        arrange(window, mtm);
        window.panel.orderFrontRegardless();
    });
}

/// Marks the rows whose meanings are known, and fills the pane beside the
/// list: a reading's other candidates, or the highlighted one's meaning.
fn mark_meanings(content: &mut Content) {
    MEANINGS.with_borrow(|meanings| {
        content.marks = content
            .rows
            .iter()
            .map(|row| row.source.is_some() && matches!(meanings.get(&row.surface), Some(Some(_))))
            .collect();
        let meaning = content
            .highlighted
            .as_deref()
            .and_then(|surface| meanings.get(surface).flatten());
        content.meaning = pane_text(&content.more, meaning);
    });
}

/// Keeps the meaning of `surface` looked up, and draws it when its page is
/// still shown.
pub fn meaning_found(surface: String, meaning: Option<String>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if !MEANINGS.with_borrow_mut(|m| m.answer(surface, meaning)) {
        return;
    }
    WINDOW.with_borrow_mut(|slot| {
        let Some(window) = slot else { return };
        if !window.panel.isVisible() {
            return;
        }
        mark_meanings(&mut window.list.ivars().borrow_mut());
        arrange(window, mtm);
    });
}

pub fn hide() {
    if let Some(page) = MEANINGS.with_borrow_mut(|m| m.want(Vec::new())) {
        meaning::ask(page);
    }
    WINDOW.with_borrow(|slot| {
        if let Some(window) = slot
            && window.panel.isVisible()
        {
            window.panel.orderOut(None);
        }
    });
}

/// Lays the content out, sizes the window to it and places it by the caret.
fn arrange(window: &Window, mtm: MainThreadMarker) {
    STYLE.with(|style| {
        let mut content = window.list.ivars().borrow_mut();
        let measure = |text: &str, attributes: &NSDictionary<NSString, AnyObject>| unsafe {
            NSString::from_str(text).sizeWithAttributes(Some(attributes))
        };
        let widths: Vec<RowWidths> = content
            .rows
            .iter()
            .map(|row| RowWidths {
                number: measure(&row.number, &style.number).width.ceil(),
                surface: measure(&row.surface, &style.surface).width.ceil(),
                source: row
                    .beside()
                    .map_or(0.0, |beside| measure(beside, &style.source).width.ceil()),
            })
            .collect();
        content.row_height = (measure("あ", &style.surface).height + ROW_SPACE).ceil();
        let meaning = content.meaning.as_deref().map(|meaning| {
            let bounds = unsafe {
                NSString::from_str(meaning).boundingRectWithSize_options_attributes_context(
                    NSSize::new(MEANING_WIDTH, MEANING_HEIGHT),
                    meaning_options(),
                    Some(&style.meaning),
                    None,
                )
            };
            Size {
                width: bounds.size.width.ceil().min(MEANING_WIDTH),
                height: bounds.size.height.ceil().min(MEANING_HEIGHT),
            }
        });
        let footer = content.footer.as_deref().map(|footer| {
            let size = measure(footer, &style.source);
            Size {
                width: size.width.ceil(),
                height: size.height.ceil(),
            }
        });
        content.layout = layout(&widths, content.row_height, meaning, footer);
    });
    let size = window.list.ivars().borrow().layout.size;
    let (x, y) = place(window.caret, size, screen_of(window.caret, mtm));
    window.panel.setFrame_display(
        NSRect::new(NSPoint::new(x, y), NSSize::new(size.width, size.height)),
        true,
    );
    window.list.setNeedsDisplay(true);
    window.panel.invalidateShadow();
}

/// The part of the screen the caret is on that windows may use.
fn screen_of(caret: Rect, mtm: MainThreadMarker) -> Rect {
    let on = |frame: NSRect| {
        (frame.origin.x..frame.origin.x + frame.size.width).contains(&caret.x)
            && (frame.origin.y..frame.origin.y + frame.size.height).contains(&caret.y)
    };
    let screens = NSScreen::screens(mtm);
    let screen = screens
        .iter()
        .find(|screen| on(screen.frame()))
        .or_else(|| NSScreen::mainScreen(mtm));
    match screen {
        Some(screen) => rect(screen.visibleFrame()),
        // Nowhere to keep it inside of: placed by the caret alone.
        None => Rect {
            x: f64::MIN / 2.0,
            y: f64::MIN / 2.0,
            width: f64::MAX,
            height: f64::MAX,
        },
    }
}

/// The line of the start of the marked text in screen coordinates, as the
/// client reports it.
fn caret_rect(client: &AnyObject) -> Option<Rect> {
    let mut line = NSRect::ZERO;
    let _: Option<Retained<NSDictionary>> = unsafe {
        msg_send![client, attributesForCharacterIndex: 0usize, lineHeightRectangle: &mut line]
    };
    if line.size.height > 0.0 {
        return Some(rect(line));
    }
    // This one takes a place in the whole document, not in the marked text.
    let marked: NSRange = unsafe { msg_send![client, markedRange] };
    let start = if marked.location == NSNotFound as usize {
        let selected: NSRange = unsafe { msg_send![client, selectedRange] };
        selected.location
    } else {
        marked.location
    };
    let mut actual = NSRange::new(0, 0);
    let first: NSRect = unsafe {
        msg_send![client, firstRectForCharacterRange: NSRange::new(start, 0), actualRange: &mut actual]
    };
    (first.size.height > 0.0).then(|| rect(first))
}
