// SPDX-License-Identifier: GPL-3.0-only
//! The gated drafts test against the live harness: the drafts family exercised against
//! a real Drafts folder, over the same accepted-certificate dial the list and reading
//! tests use.
//!
//! Each step is a contract the family owns: a Save renders the composer's document and
//! appends it to the account's Drafts folder over the dial, the engine store presents it
//! to the next pass, and the resume read opens its words back; a Discard expunges the
//! stored copy so the resume read finds nothing; a Close leaves what the composer held
//! on the server and forgets the composition, so the words are still there to be
//! resumed. The harness is online, so every put lands directly rather than parking in
//! the outbox.

use std::sync::Arc;

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use astra_viewmodel::DraftSaveState;
use engine_api::{EmailAddress, Engine};

use crate::{Account, App, AppObserver, CompositionId, DraftsIntent, Intent};

use super::{dial_with_accepted_certificate, harness};

/// A composition id, the way the drafts test names one.
fn composition(value: &str) -> CompositionId {
    CompositionId::new(value).expect("a composition id")
}

/// A one-paragraph document carrying `text`, the smallest document a composer hands
/// over; the plain words of the paragraph are what the resume read expects back.
fn document(text: &str) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![Block::Paragraph(Paragraph {
            content: vec![InlineContent::Text(TextRun {
                text: text.to_owned(),
                bold: false,
                italic: false,
                underline: false,
                font_size: None,
                color: None,
                highlight: None,
                link: None,
            })],
        })],
        attachments: Vec::new(),
    }
}

/// The drafts family against the live harness: a save stores the composer's words in the
/// account's Drafts folder and the store presents them to the resume read; a discard
/// removes the stored copy so nothing is left to resume; a save followed by a close
/// leaves the draft in Drafts, forgotten by the core but still the words the next resume
/// opens.
///
/// Like the list test, the run first puts the account back to a known state through the
/// product's own paths: a warm volume may hold a draft an earlier run left in Drafts,
/// and "nothing left to resume" is only provable from an account that started empty.
#[tokio::test]
async fn the_drafts_family_saves_discards_and_closes_through_the_live_store() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (account_id, providers, ..) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;

    let observer: Arc<dyn AppObserver> = Arc::new(super::CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account_id.clone(),
            providers,
            identity: EmailAddress::new("astra1@example.test"),
        }],
        None,
        observer,
    );

    // Back to a known state: each draft an earlier run left is adopted by the resume
    // read and discarded through the composition it opened, until the account's Drafts
    // folder holds none the store presents.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let mut swept = 0;
    while app
        .resume_latest_draft(&account_id, composition("sweep"))
        .await
        .is_some()
    {
        app.dispatch(Intent::Drafts(DraftsIntent::Discard {
            composition: composition("sweep"),
        }))
        .await
        .unwrap();
        app.dispatch(Intent::RefreshMail).await.unwrap();
        swept += 1;
        assert!(
            swept < 8,
            "the sweep is discarding more drafts than any earlier run leaves"
        );
    }

    // The save: the put lands on the server at once (the harness is online), and the
    // composition's hint says so.
    app.dispatch(Intent::Drafts(DraftsIntent::Save {
        composition: composition("live-1"),
        from: account_id.clone(),
        to: "astra2@example.test".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "Harness draft".to_owned(),
        document: document("Half written harness words"),
        then_close: false,
    }))
    .await
    .unwrap();
    let status = app
        .draft_status()
        .expect("the landed save reports its outcome");
    assert_eq!(
        status.state,
        DraftSaveState::Saved,
        "the online put lands directly rather than parking in the outbox"
    );

    // The store holds it once the Drafts folder's next pass presents it, and the resume
    // read opens the words back.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let stored = app
        .engine
        .messages(&account_id)
        .await
        .unwrap()
        .into_iter()
        .filter(engine_api::Message::is_draft)
        .count();
    assert_eq!(stored, 1, "the store holds exactly the saved draft");
    let resumed = app
        .resume_latest_draft(&account_id, composition("probe-1"))
        .await
        .expect("the saved draft resumes");
    assert_eq!(resumed.subject, "Harness draft");
    assert_eq!(resumed.to, "astra2@example.test");
    assert!(
        resumed.body_text.contains("Half written harness words"),
        "the resumed body is the words that were saved: {}",
        resumed.body_text
    );

    // The discard: the stored copy leaves the server and the composition its hint.
    app.dispatch(Intent::Drafts(DraftsIntent::Discard {
        composition: composition("live-1"),
    }))
    .await
    .unwrap();
    assert!(
        app.draft_status().is_none(),
        "the discarded composition leaves no hint behind"
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.resume_latest_draft(&account_id, composition("probe-2"))
            .await
            .is_none(),
        "no draft is left to resume after the discard"
    );

    // A second save, then the close: the close keeps what the composer handed over (the
    // same words are already stored, so no second write) and forgets the composition.
    app.dispatch(Intent::Drafts(DraftsIntent::Save {
        composition: composition("live-2"),
        from: account_id.clone(),
        to: "astra2@example.test".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "Harness kept draft".to_owned(),
        document: document("Kept on the way out"),
        then_close: false,
    }))
    .await
    .unwrap();
    app.dispatch(Intent::Drafts(DraftsIntent::Close {
        composition: composition("live-2"),
    }))
    .await
    .unwrap();
    assert!(
        app.draft_status().is_none(),
        "the close forgets the composition, hint and all"
    );

    // And the words survive: the next pass presents the copy the close left in Drafts,
    // and the resume read opens it once more.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let resumed = app
        .resume_latest_draft(&account_id, composition("probe-3"))
        .await
        .expect("the closed composer's draft is still in Drafts");
    assert_eq!(resumed.subject, "Harness kept draft");
    assert!(
        resumed.body_text.contains("Kept on the way out"),
        "the resumed body is the content the close kept: {}",
        resumed.body_text
    );
}
