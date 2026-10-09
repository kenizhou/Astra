// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the drafts commands: the status pull follows the saves'
//! transitions and empties when the composition is forgotten, and the resume
//! read answers with the stored draft until a discard removes it. The fake app
//! and the state builder live in `commands_fakes`, one compiled copy for every
//! command test module.

use std::sync::Arc;

use astra_viewmodel::{DraftSaveState, DraftStatusSnapshot};

use super::{draft_resume_of, draft_status_of, ResumeView};
use crate::app_state::AppState;
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The save the webview sends, shaped like the ipc layer's constructor output: a
/// one-paragraph document with one plain run, the same document the app core's
/// protocol tests pin.
fn save_json() -> serde_json::Value {
    serde_json::json!({
        "drafts": {"save": {
            "composition": "compose-1",
            "from": "work",
            "to": "you@test.local",
            "cc": "",
            "bcc": "",
            "subject": "Later",
            "document": {
                "blocks": [{"Paragraph": {"content": [
                    {"Text": {"text": "Half written", "bold": false, "italic": false,
                              "underline": false, "font_size": null, "color": null,
                              "highlight": null, "link": null}}
                ]}}],
                "attachments": []
            },
            "then_close": false
        }}
    })
}

/// Waits until the status pull answers `want`, then asserts it settled there:
/// the dispatches run on the runtime, so the test meets them where they land.
fn wait_for_status(state: &AppState, want: Option<&DraftStatusSnapshot>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while draft_status_of(state).as_ref() != want && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        draft_status_of(state),
        want.cloned(),
        "the pull settles on the app's published snapshot",
    );
}

#[test]
fn the_status_pull_follows_the_saves_and_empties_when_the_composition_is_forgotten() {
    let app = FakeApp::new();
    let state = state("drafts-status", Arc::clone(&app), FakeStore::accepting());

    // Nothing has saved: the pull answers `None`, never a panic.
    assert_eq!(draft_status_of(&state), None);

    // Save through the same JSON the webview sends; the pull settles on the
    // landed hint the app published.
    crate::commands::dispatch_intent(&state, save_json()).expect("the save dispatches");
    let saved = DraftStatusSnapshot {
        composition: "compose-1".to_owned(),
        state: DraftSaveState::Saved,
        saved_at: "2026-10-09T08:30:00Z".to_owned(),
    };
    wait_for_status(&state, Some(&saved));

    // Close the composition the same way; the hint goes with it, and the pull
    // answers `None` again.
    crate::commands::dispatch_intent(
        &state,
        serde_json::json!({"drafts": {"close": {"composition": "compose-1"}}}),
    )
    .expect("the close dispatches");
    wait_for_status(&state, None);
}

#[test]
fn the_resume_read_answers_the_stored_draft_and_none_after_a_discard() {
    let app = FakeApp::new();
    let state = state("drafts-resume", Arc::clone(&app), FakeStore::accepting());

    // A stored draft to resume: the save has to land before the read runs.
    crate::commands::dispatch_intent(&state, save_json()).expect("the save dispatches");
    let saved = DraftStatusSnapshot {
        composition: "compose-1".to_owned(),
        state: DraftSaveState::Saved,
        saved_at: "2026-10-09T08:30:00Z".to_owned(),
    };
    wait_for_status(&state, Some(&saved));

    // The composition is the caller's own: minted for the composer about to
    // open, exactly as for a fresh message.
    let account = engine_api::AccountId::try_from("work").expect("an account id");
    let composition = astra_app::CompositionId::new("compose-9").expect("a composition id");
    let resumed = state
        .runtime
        .block_on(|| draft_resume_of(&state, &account, composition.clone()));
    assert_eq!(
        resumed,
        Some(ResumeView {
            account: "work".to_owned(),
            to: "you@test.local".to_owned(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Later".to_owned(),
            body_text: "Half written".to_owned(),
        }),
        "the resume answers with the stored draft, document rendered to plain text",
    );
    assert_eq!(
        app.resumed_into.lock().unwrap().as_slice(),
        [("work".to_owned(), "compose-9".to_owned())].as_slice(),
        "the command hands the webview's account and composition to the core",
    );

    // A discard removes the stored draft; the resume answers `None`.
    crate::commands::dispatch_intent(
        &state,
        serde_json::json!({"drafts": {"discard": {"composition": "compose-1"}}}),
    )
    .expect("the discard dispatches");
    wait_for_status(&state, None);
    let after = state
        .runtime
        .block_on(|| draft_resume_of(&state, &account, composition));
    assert_eq!(after, None, "a discarded draft is gone from the account");
}

#[test]
fn the_resume_view_crosses_as_one_flat_record() {
    let view = ResumeView {
        account: "work".to_owned(),
        to: "you@test.local".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "Later".to_owned(),
        body_text: "Half written".to_owned(),
    };
    let wire = serde_json::to_value(&view).expect("the view serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "account": "work",
            "to": "you@test.local",
            "cc": "",
            "bcc": "",
            "subject": "Later",
            "body_text": "Half written"
        }),
    );
}
