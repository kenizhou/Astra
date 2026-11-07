// SPDX-License-Identifier: GPL-3.0-only
//! The gated drafts-reopen proof against the live harness: the round trip the
//! Drafts row owns, over the account's real Drafts folder and the same
//! accepted-certificate dial the drafts suite uses. A draft carrying one
//! attachment is stored through the engine's own put path (the Draft type
//! carries attachments; provider-imap assembles them into the stored MIME),
//! the row's key is found through the product's own list, the by-key resume
//! answers the fields and the carried rows, and the adopted composition's next
//! save supersedes the stored copy rather than storing a second beside it. The
//! three refusal gates and the wire shapes are pinned offline
//! (resume_by_key_tests); this proof pins the real-server round trip. The run
//! sweeps the Drafts folder back to empty at both ends, the shape the drafts
//! suite leaves and expects.

use std::sync::Arc;

use astra_composer::ComposerDocument;
use astra_viewmodel::{DraftSaveState, FolderRole, RowKey};
use engine_api::{Draft, EmailAddress, Engine, MessageIdHeader};

use crate::{
    reference::MessageRef, Account, App, AppObserver, CompositionId, DraftsIntent, FolderRef,
    Intent,
};

use super::{dial_with_accepted_certificate, harness, proof, row_by_subject};

/// The subject the first stored copy carries: unique among the harness fixtures.
const SUBJECT: &str = "Harness reopen proof";

/// The subject the superseding save carries, so the replace is observable.
const EDITED: &str = "Harness reopen proof edited";

/// The words the first stored copy carries.
const WORDS: &str = "Half written words the reopen round trip opens back.";

/// The words the superseding save carries.
const EDITED_WORDS: &str = "Edited words the save stores over the first copy.";

/// The file name the one attachment carries.
const FILE: &str = "reopen-notes.txt";

/// The attachment's bytes, stored with the draft and read back with it.
const BYTES: &[u8] = b"harness reopen proof bytes 0123456789";

/// A composition id, the way the drafts tests name one.
fn composition(value: &str) -> CompositionId {
    CompositionId::new(value).expect("a composition id")
}

/// The Drafts folder of `account`, as the pane projects it.
fn drafts_folder(
    app: &App<Box<dyn engine_api::Provider>>,
    account: &engine_api::AccountId,
) -> FolderRef {
    FolderRef::from_parts(
        account.as_str(),
        proof::role_key(app, account, FolderRole::Drafts),
    )
    .expect("the Drafts folder reference")
}

/// The Drafts row whose subject matches, through the product's own list.
async fn draft_row_key(
    app: &App<Box<dyn engine_api::Provider>>,
    account: &engine_api::AccountId,
    subject: &str,
) -> MessageRef {
    app.dispatch(Intent::SelectFolder {
        folder: drafts_folder(app, account),
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let key = row_by_subject(&snapshot, subject).key.clone();
    let RowKey::Message { account, key } = key else {
        panic!("a flat list row is keyed by its message");
    };
    MessageRef { account, key }
}

/// The drafts the account's store holds.
async fn stored_drafts(
    app: &App<Box<dyn engine_api::Provider>>,
    account: &engine_api::AccountId,
) -> Vec<engine_api::Message> {
    app.engine
        .messages(account)
        .await
        .expect("the account's stored messages")
        .into_iter()
        .filter(engine_api::Message::is_draft)
        .collect()
}

/// The reopen round trip live: a stored draft's row resumes by its own key
/// with fields and carried parts answering, and the adopted composition's save
/// supersedes the same copy, leaving one draft, not two.
#[tokio::test]
async fn the_drafts_row_reopens_and_saves_over_the_same_copy() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (account, providers, ..) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;
    let observer: Arc<dyn AppObserver> = Arc::new(super::CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account.clone(),
            providers,
            identity: EmailAddress::new(&user),
        }],
        None,
        observer,
    );

    // Back to a known state: the drafts suite's own opening sweep, so the
    // account's Drafts folder holds nothing an earlier run left.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let mut swept = 0;
    while app
        .resume_latest_draft(&account, composition("sweep"))
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

    // The store: a draft carrying one attachment through the engine's own put
    // path, the same verb a save rides.
    let handle = app
        .account_handle(&account)
        .await
        .expect("the account the proof dialed");
    let provider = handle
        .providers
        .first()
        .expect("the provider to store through");
    let draft = Draft::new(
        MessageIdHeader::new("harness-reopen-proof@astra.local").expect("the message id"),
        EmailAddress::new(&user),
        vec![EmailAddress::new(proof::RECIPIENT)],
        SUBJECT.to_owned(),
        WORDS,
    )
    .with_attachment(engine_api::DraftAttachment::attachment(
        FILE.to_owned(),
        "text/plain".to_owned(),
        BYTES.to_vec(),
    ));
    let outcome = app
        .engine
        .put_draft(provider, &account, &draft, None)
        .await
        .expect("the storing put");

    // The row's key through the product's own list, after the pass that
    // presents the stored copy.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let row = draft_row_key(&app, &account, SUBJECT).await;
    assert_eq!(
        row.key, outcome.key,
        "the listed row is the copy the put stored"
    );

    // The by-key resume: the fields and the carried rows answer.
    let resumed = app
        .resume_draft_by_key(&row, composition("reopen"))
        .await
        .expect("the named copy resumes");
    assert_eq!(resumed.subject, SUBJECT);
    assert_eq!(resumed.to, proof::RECIPIENT);
    assert!(
        resumed.body_text.contains(WORDS),
        "the resumed body is the words that were stored: {}",
        resumed.body_text
    );
    assert_eq!(
        resumed.attachments.len(),
        1,
        "the stored part answers as one pre-attached row"
    );
    assert_eq!(resumed.attachments[0].file_name, FILE);
    assert!(
        resumed.attachments[0].blob.is_some(),
        "the row carries a handle naming the parked part, never bytes"
    );

    // The save through the adopted composition: an edited subject over the
    // same document rows, the shape a composer hands back after a resume.
    app.dispatch(Intent::Drafts(DraftsIntent::Save {
        composition: composition("reopen"),
        from: account.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: EDITED.to_owned(),
        document: ComposerDocument {
            blocks: vec![proof::paragraph(EDITED_WORDS)],
            attachments: resumed.attachments.clone(),
        },
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
        "the online superseding save lands directly"
    );

    // The supersession: the store holds exactly the one edited copy, never a
    // second beside the first.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let stored = stored_drafts(&app, &account).await;
    assert_eq!(
        stored.len(),
        1,
        "the save superseded the stored copy rather than storing a second beside it"
    );
    assert_eq!(
        stored[0].envelope.subject.as_deref(),
        Some(EDITED),
        "the one stored draft is the edited copy"
    );

    // The cleanup: the composition's discard removes the stored copy, the
    // Drafts folder back to the empty shape the suites expect.
    app.dispatch(Intent::Drafts(DraftsIntent::Discard {
        composition: composition("reopen"),
    }))
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let left = stored_drafts(&app, &account).await;
    assert!(
        left.is_empty(),
        "the discard leaves the account holding no draft"
    );
}
