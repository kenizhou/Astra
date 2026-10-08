// SPDX-License-Identifier: GPL-3.0-only
//! The mailbox-list paging window: how many rows the list shows at once. The sizing
//! policy lives on its own so the runtime's state stays separate from it; the constant
//! stays here beside the policy that reads it.

use crate::App;

/// How many mailbox-list rows one page holds: the initial window after any navigation,
/// and the step [`Intent::ShowMore`](crate::Intent::ShowMore) grows it by. Sized so the
/// first screen builds and crosses the IPC fast (the rest load as the host scrolls).
pub(crate) const PAGE: usize = 100;

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
}
