// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the attachment browser command: the pull answers the view the
//! core holds and threads the filter legs through whole, and a blank filter
//! leg narrows nothing (it crosses as absent, never as a match-nothing
//! string).

use std::sync::Arc;

use astra_viewmodel::{
    AttachmentBrowserEntry, AttachmentBrowserFilter, AttachmentBrowserView, MessageKey,
};
use engine_api::{AccountId, ProviderKey};

use super::attachment_browser_of;
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// One browser row, the shape the pull answers with.
fn entry() -> AttachmentBrowserEntry {
    AttachmentBrowserEntry {
        id: 2,
        file_name: "report.pdf".to_owned(),
        media_type: "application/pdf".to_owned(),
        size: 3,
        source: MessageKey {
            account: AccountId::try_from("work").unwrap(),
            key: ProviderKey::new("m1").unwrap(),
        },
        subject: "Launch".to_owned(),
        from_name: "Ada Lovelace".to_owned(),
        from_address: "ada@example.test".to_owned(),
        sent_at: "2026-10-09T08:00:00Z".to_owned(),
    }
}

#[test]
fn the_pull_answers_the_view_and_threads_the_filter() {
    let app = FakeApp::new();
    let view = AttachmentBrowserView {
        entries: vec![entry()],
        ..AttachmentBrowserView::default()
    };
    app.browser.lock().unwrap().view = view.clone();
    let state = state("browser-ok", Arc::clone(&app), FakeStore::accepting());

    let answered = state.runtime.block_on(|| {
        attachment_browser_of(
            &state,
            Some("ada@example.test".to_owned()),
            Some("image/".to_owned()),
        )
    });

    assert_eq!(answered, view, "the core's view crosses untouched");
    let asked = app.browser.lock().unwrap();
    assert_eq!(asked.asked.len(), 1, "one browse reaches the core");
    assert_eq!(asked.asked[0].sender.as_deref(), Some("ada@example.test"));
    assert_eq!(asked.asked[0].mime_prefix.as_deref(), Some("image/"));
}

#[test]
fn blank_filter_legs_narrow_nothing() {
    let app = FakeApp::new();
    let state = state("browser-blank", Arc::clone(&app), FakeStore::accepting());

    let _ = state
        .runtime
        .block_on(|| attachment_browser_of(&state, Some("   ".to_owned()), Some(String::new())));

    let asked = app.browser.lock().unwrap();
    assert_eq!(
        asked.asked[0],
        AttachmentBrowserFilter::default(),
        "a blank leg crosses as absent, never a match-nothing string",
    );
}
