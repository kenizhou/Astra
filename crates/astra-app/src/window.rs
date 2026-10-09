// SPDX-License-Identifier: GPL-3.0-only
//! The mailbox-list paging window: how many rows the list shows at once, and how deep
//! the store read that backs them goes. The sizing policy lives on its own so the
//! runtime's state stays separate from it; the constants stay here beside the policy
//! that reads them.

use crate::App;

/// How many mailbox-list rows one page holds: the initial window after any navigation,
/// and the step [`Intent::ShowMore`](crate::Intent::ShowMore) grows it by. Sized so the
/// first screen builds and crosses the IPC fast (the rest load as the host scrolls).
pub(crate) const PAGE: usize = 100;

/// How many of the account's newest messages a snapshot reads from the store at the very
/// least: a floor that keeps the first page of a folder full even when the account's
/// newest mail is filed elsewhere.
pub(crate) const MIN_LOAD_WINDOW: usize = 500;

/// How much deeper than the visible window the store read goes: a folder's rows are a
/// slice of the account's newest mail, so the read over-reaches to keep one folder's
/// page from running short of the window's own depth.
pub(crate) const WINDOW_ROW_FACTOR: usize = 4;

impl<P> App<P> {
    /// Resets the visible mailbox-list window to the first [`PAGE`]; called on any
    /// navigation that changes which list is shown, so it always opens at the top.
    pub(crate) fn reset_window(&self) {
        *self.visible_limit.lock().expect("window mutex poisoned") = PAGE;
    }

    /// Grows the visible window by one [`PAGE`]: the host's "scrolled near the end,
    /// show more" step. It can grow past the row count harmlessly; the projection just
    /// returns every row, so the host stops asking once it has them all.
    pub(crate) fn grow_window(&self) {
        let mut limit = self.visible_limit.lock().expect("window mutex poisoned");
        *limit = limit.saturating_add(PAGE);
    }

    /// The current visible window size, for
    /// [`rebuild_snapshot`](App::rebuild_snapshot).
    pub(crate) fn visible_limit(&self) -> usize {
        *self.visible_limit.lock().expect("window mutex poisoned")
    }

    /// How many of the account's newest messages a snapshot reads from the store: the
    /// message-load window. Scales with the visible row limit (so
    /// [`Intent::ShowMore`](crate::Intent::ShowMore) loads deeper) over a floor of
    /// [`MIN_LOAD_WINDOW`], so the window bounds only which conversations appear; the
    /// threaded view still completes a shown conversation's older members from the
    /// store's thread index.
    pub(crate) fn load_window(&self) -> usize {
        self.visible_limit()
            .saturating_mul(WINDOW_ROW_FACTOR)
            .max(MIN_LOAD_WINDOW)
    }
}
