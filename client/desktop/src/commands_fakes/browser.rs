// SPDX-License-Identifier: GPL-3.0-only
//! The attachment browser pull's fake state: the view the pull answers with
//! and the filters it was asked with, under one lock so a pull and its
//! answer cannot tear. A child of the fixture root for the line cap.

use astra_viewmodel::{AttachmentBrowserFilter, AttachmentBrowserView};

/// The browser pull's seed and record.
#[derive(Default)]
pub(crate) struct BrowserFake {
    /// The view the pull answers with.
    pub(crate) view: AttachmentBrowserView,
    /// The filters the pull was asked with, in order.
    pub(crate) asked: Vec<AttachmentBrowserFilter>,
}
