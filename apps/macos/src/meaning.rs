//! Looks candidates up in the dictionaries of macOS (Dictionary Services)
//! on a thread of its own: a lookup may read the disk or take long the
//! first time, and keys are never kept waiting for it.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr;
use std::sync::mpsc::{self, Sender};

use dispatch2::DispatchQueue;
use objc2::rc::{Retained, autoreleasepool};
use objc2_foundation::NSString;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CFRange {
    location: isize,
    length: isize,
}

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    // CFStringRef is toll-free bridged with NSString.
    fn DCSGetTermRangeInString(
        dictionary: *const c_void,
        text: *const NSString,
        offset: isize,
    ) -> CFRange;
    fn DCSCopyTextDefinition(
        dictionary: *const c_void,
        text: *const NSString,
        range: CFRange,
    ) -> *mut NSString;
}

thread_local! {
    /// Where the candidates to look up go, to the thread that looks them up.
    static ASK: RefCell<Option<Sender<Vec<String>>>> = const { RefCell::new(None) };
}

/// Starts the thread that looks candidates up; each answer is handed to
/// `answered` on the main queue, with the meaning or `None` when there is
/// no entry for the candidate.
pub fn start(answered: fn(String, Option<String>)) {
    let (ask, asked) = mpsc::channel::<Vec<String>>();
    let spawned = std::thread::Builder::new()
        .name("meanings".to_owned())
        .spawn(move || {
            while let Ok(mut page) = asked.recv() {
                let mut next = 0;
                while let Some(surface) = page.get(next).cloned() {
                    // A page asked for later takes the place of this one,
                    // which is no longer shown.
                    if let Some(later) = asked.try_iter().last() {
                        page = later;
                        next = 0;
                        continue;
                    }
                    next += 1;
                    let meaning = define(&surface);
                    DispatchQueue::main().exec_async(move || answered(surface, meaning));
                }
            }
        });
    match spawned {
        Ok(_) => ASK.set(Some(ask)),
        Err(error) => tracing::warn!(%error, "meanings not looked up"),
    }
}

/// Asks for the meanings of `page`, in order, in place of those asked for
/// before; none waits for them.
pub fn ask(page: Vec<String>) {
    ASK.with_borrow(|ask| {
        if let Some(ask) = ask {
            let _ = ask.send(page);
        }
    });
}

/// The entry for `surface` as a whole in the dictionaries the user turned on
/// in the Dictionary app. A word found only in part of it (汽車 in 汽車で)
/// is not its meaning.
fn define(surface: &str) -> Option<String> {
    autoreleasepool(|_| {
        let text = NSString::from_str(surface);
        let whole = CFRange {
            location: 0,
            length: text.length() as isize,
        };
        // SAFETY: a null dictionary searches the user's; `text` lives
        // through the calls.
        let term = unsafe { DCSGetTermRangeInString(ptr::null(), &*text, 0) };
        if term != whole {
            return None;
        }
        // SAFETY: as above; a Copy function hands over the reference it returns.
        let found =
            unsafe { Retained::from_raw(DCSCopyTextDefinition(ptr::null(), &*text, whole)) }?;
        Some(found.to_string())
    })
}
