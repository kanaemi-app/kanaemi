//! The candidate list as a TSF UI element. An application can draw the
//! candidates itself, as the Start menu's search box does: it reads them from
//! the element and tells the text service to show no window of its own.

use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::*;
use windows::Win32::UI::TextServices::*;
use windows::core::*;

use crate::candidates;
use crate::listing::Page;

/// The page of candidates the element lists.
#[derive(Default)]
pub struct Listed {
    pub page: Page,
    /// Where the composition is on the screen, and whose window it is in.
    pub at: RECT,
    pub owner: Option<HWND>,
    /// Whether the text service's own window shows them; the application
    /// says no when it draws them itself.
    pub shown: bool,
}

impl Listed {
    /// Shows or hides the text service's own window to match.
    pub fn draw(&self) {
        if self.shown {
            candidates::show(self.page.clone(), self.at, self.owner);
        } else {
            candidates::hide();
        }
    }
}

#[implement(
    ITfCandidateListUIElementBehavior,
    ITfCandidateListUIElement,
    ITfUIElement
)]
pub struct CandidateList {
    listed: Rc<RefCell<Listed>>,
    document: ITfDocumentMgr,
}

impl CandidateList {
    pub fn new(listed: Rc<RefCell<Listed>>, document: ITfDocumentMgr) -> Self {
        Self { listed, document }
    }
}

/// Everything about the list may have changed with each update.
const UPDATED: u32 = TF_CLUIE_DOCUMENTMGR
    | TF_CLUIE_COUNT
    | TF_CLUIE_SELECTION
    | TF_CLUIE_STRING
    | TF_CLUIE_PAGEINDEX
    | TF_CLUIE_CURRENTPAGE;

impl ITfUIElement_Impl for CandidateList_Impl {
    fn GetDescription(&self) -> Result<BSTR> {
        Ok(BSTR::from("候補"))
    }

    fn GetGUID(&self) -> Result<GUID> {
        Ok(crate::com::CANDIDATE_LIST_ELEMENT)
    }

    fn Show(&self, show: BOOL) -> Result<()> {
        let mut listed = self.listed.borrow_mut();
        listed.shown = show.as_bool();
        listed.draw();
        Ok(())
    }

    fn IsShown(&self) -> Result<BOOL> {
        Ok(self.listed.borrow().shown.into())
    }
}

impl ITfCandidateListUIElement_Impl for CandidateList_Impl {
    fn GetUpdatedFlags(&self) -> Result<u32> {
        Ok(UPDATED)
    }

    fn GetDocumentMgr(&self) -> Result<ITfDocumentMgr> {
        Ok(self.document.clone())
    }

    fn GetCount(&self) -> Result<u32> {
        Ok(self.listed.borrow().page.items.len() as u32)
    }

    fn GetSelection(&self) -> Result<u32> {
        Ok(self.listed.borrow().page.selected as u32)
    }

    fn GetString(&self, index: u32) -> Result<BSTR> {
        self.listed
            .borrow()
            .page
            .items
            .get(index as usize)
            .map(|item| BSTR::from(item.as_str()))
            .ok_or_else(|| E_INVALIDARG.into())
    }

    /// The core shows one page at a time: the list is that one page.
    fn GetPageIndex(&self, index: *mut u32, size: u32, count: *mut u32) -> Result<()> {
        if count.is_null() {
            return Err(E_INVALIDARG.into());
        }
        unsafe { *count = 1 };
        if !index.is_null() && size >= 1 {
            unsafe { *index = 0 };
        }
        Ok(())
    }

    fn SetPageIndex(&self, _index: *const u32, _count: u32) -> Result<()> {
        Ok(())
    }

    fn GetCurrentPage(&self) -> Result<u32> {
        Ok(0)
    }
}

/// Choosing from the application's own list goes through the keys, which
/// the core already takes; these are accepted and do nothing.
impl ITfCandidateListUIElementBehavior_Impl for CandidateList_Impl {
    fn SetSelection(&self, _index: u32) -> Result<()> {
        Ok(())
    }

    fn Finalize(&self) -> Result<()> {
        Ok(())
    }

    fn Abort(&self) -> Result<()> {
        Ok(())
    }
}
