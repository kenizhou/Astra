// SPDX-License-Identifier: GPL-3.0-only
//! The gated watch-replies proof against the live harness: a submitted note
//! watched through the core, a reply delivered over the real wire whose
//! Message-ID sorts BEFORE the original's (so the engine re-keys the derived
//! thread onto the reply's id), the post-pass check still firing one notice
//! naming the reply (the member-key re-resolution survives the re-key), the
//! watch row gone (one-shot), a second reply raising nothing, and both
//! messages still in the INBOX unmoved (a watch owns no engine state). The
//! copies are withdrawn so the harness keeps its seeded shape.
//!
//! The reply rides the harness's raw SMTP submission listener (the reading
//! proof's wire) so the proof can CHOOSE its Message-ID: the product's own
//! submit mints a wall-clock `draft-…` id that would sort after the original,
//! hiding the re-key. A fixed `aaa-…` id sorts before any `draft-…` id, so the
//! merge always re-keys, and the proof pins that it did.

use std::sync::Arc;

use astra_watches::WatchStore;

use crate::crypto::CryptoFlags;
use crate::reference::MessageRef;
use crate::{FolderRef, Intent, RowRef};

use super::{
    harness, inbox_key,
    proof::{self, document},
};

/// The submitted note's subject: unique among the harness fixtures.
const ORIGINAL_SUBJECT: &str = "Harness watch replies proof";

/// The reply's subject: the answer the check notifies about.
const REPLY_SUBJECT: &str = "Re: Harness watch replies proof";

/// The second reply's subject: the one-shot's silence after the consume.
const SECOND_SUBJECT: &str = "Re: Harness watch replies proof (second)";

/// The reply's Message-ID, chosen to sort BEFORE the original's minted
/// `draft-…` id ('a' < 'd'), so the derived thread re-keys onto it on merge.
const REPLY_MSGID: &str = "aaa-watch-reply@example.test";

/// The second reply's Message-ID: still before `draft-…`, after the first
/// reply's, so the thread keeps the first reply's name and the consumed watch
/// stays silent.
const SECOND_MSGID: &str = "bbb-watch-reply-second@example.test";

/// The words the submitted note carries.
const WORDS: &str = "The original note the watch replies proof answers.";

/// The words the reply carries.
const REPLY_WORDS: &str = "The reply the watched conversation receives.";

/// The subjects the proof sweeps and withdraws.
const SUBJECTS: [&str; 3] = [ORIGINAL_SUBJECT, REPLY_SUBJECT, SECOND_SUBJECT];

/// Builds one raw reply: from astra1 to astra2, owning `msgid` and answering
/// `in_reply_to` (its In-Reply-To and References both name the original), so
/// the engine threads it under the original on delivery.
fn reply_raw(subject: &str, msgid: &str, in_reply_to: &str, words: &str) -> String {
    format!(
        "From: Astra One <astra1@example.test>\r\n\
         To: <astra2@example.test>\r\n\
         Subject: {subject}\r\n\
         Date: Sat, 24 Oct 2026 11:00:00 +0000\r\n\
         Message-ID: <{msgid}>\r\n\
         In-Reply-To: <{in_reply_to}>\r\n\
         References: <{in_reply_to}>\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         {words}\r\n"
    )
}

/// Submits `message`'s raw bytes from astra1 to astra2 through the harness's
/// SMTP submission listener: curl over implicit TLS from the host, the same
/// wire the reading proof rides. Seeding only; every assertion that follows
/// reads through the product.
fn submit_raw(host: &str, password: &str, message: &str) {
    let path = std::env::temp_dir().join(format!("astra-watch-proof-{}.eml", std::process::id()));
    std::fs::write(&path, message.as_bytes()).expect("the raw submission file writes");
    let output = std::process::Command::new("curl")
        .arg("-sk")
        .arg("--user")
        .arg(format!("astra1@example.test:{password}"))
        .arg("--mail-from")
        .arg("astra1@example.test")
        .arg("--mail-rcpt")
        .arg(proof::RECIPIENT)
        .arg("--url")
        .arg(format!("smtps://{host}:{}", super::harness_smtp_port()))
        .arg("-T")
        .arg(&path)
        .output()
        .expect("the raw submission spawns");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "the raw submission delivered: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The watch-replies round trip against the live harness: the row menu's own
/// verb over the real dial, a re-keying reply the check still catches, the
/// one-shot consume, the second reply's silence, and the mail left unmoved.
#[tokio::test]
async fn a_watched_reply_notifies_through_a_rekey_and_leaves_the_mail_unmoved() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;
    let store = Arc::new(WatchStore::open_in_memory().expect("the watch store"));
    app.install_watch_store(Arc::clone(&store));

    // The converging sweep: import the harness state, withdraw a prior
    // interrupted run's copies, settle the queue to a known empty.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &SUBJECTS).await;
    proof::delete_subject_mail(&app, &recipient, &SUBJECTS).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.drain_new_mail();

    // The original, made the proven way: submitted from astra1, delivered to
    // astra2 through the engine's real send path. The Write Contract mints its
    // stable Message-ID (a wall-clock `draft-…` id).
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: ORIGINAL_SUBJECT.to_owned(),
        document: document(WORDS),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
        composition: None,
    })
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.drain_new_mail();

    // The original's own Message-ID and provider key off the delivered copy.
    let original = app
        .engine
        .messages(&recipient)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.envelope.subject.as_deref() == Some(ORIGINAL_SUBJECT))
        .expect("the delivered original in the recipient's store");
    let original_msgid = original
        .envelope
        .message_id
        .first()
        .expect("the delivered original carries a Message-ID")
        .clone();
    let original_key = original.id.key().clone();

    // The watch, through the core's own verb: the conversation's thread key
    // resolved and one representative member key captured.
    app.watch_thread(RowRef::Message(MessageRef {
        account: recipient.clone(),
        key: original_key.clone(),
    }))
    .await
    .expect("the watch");
    assert_eq!(
        store.list().expect("the listing").len(),
        1,
        "the watch records"
    );

    // The reply's Message-ID sorts BEFORE the original's, so the merge re-keys
    // the derived thread onto the reply's id: the trap the member-key
    // re-resolution exists to survive.
    assert!(
        REPLY_MSGID < original_msgid.as_str(),
        "the reply's id sorts first: {REPLY_MSGID} < {}",
        original_msgid.as_str()
    );
    submit_raw(
        &host,
        &password,
        &reply_raw(
            REPLY_SUBJECT,
            REPLY_MSGID,
            original_msgid.as_str(),
            REPLY_WORDS,
        ),
    );

    // The pass + check, the host hook's own body: the account's pass lands the
    // reply, the drain takes the batch, the check answers the watched reply.
    app.sync_account(&recipient).await;
    let batch = app.drain_new_mail();
    let notices = app.check_watched_replies(&batch.messages).await;
    assert_eq!(notices.len(), 1, "the first reply raises one notice");
    assert_eq!(
        notices[0].subject.as_deref(),
        Some(REPLY_SUBJECT),
        "the notice names the reply (the bridge titles it 'Reply received')"
    );
    assert_eq!(
        notices[0].account, recipient,
        "the notice names the account the reply landed in"
    );
    assert!(
        store.list().expect("the listing").is_empty(),
        "the one-shot consume clears the watch"
    );

    // The re-key actually happened over the wire: the original's stored thread
    // id moved onto the reply's smaller Message-ID.
    let rekeyed = app
        .engine
        .mail_by_keys(&recipient, std::slice::from_ref(&original_key))
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("the original still resolves");
    assert_eq!(
        rekeyed
            .mail
            .thread_id
            .as_ref()
            .map(engine_api::ThreadId::as_str),
        Some(REPLY_MSGID),
        "the derived thread re-keyed onto the reply's smaller Message-ID"
    );

    // The second reply: the watch is consumed, so the check raises nothing.
    submit_raw(
        &host,
        &password,
        &reply_raw(
            SECOND_SUBJECT,
            SECOND_MSGID,
            original_msgid.as_str(),
            REPLY_WORDS,
        ),
    );
    app.sync_account(&recipient).await;
    let second_batch = app.drain_new_mail();
    let second_notices = app.check_watched_replies(&second_batch.messages).await;
    assert!(
        second_notices.is_empty(),
        "the one-shot watch raises nothing on the second reply"
    );

    // Both messages (and the second reply) are STILL in the INBOX, unmoved: a
    // watch owns no engine state, so nothing left its folder.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let subjects: Vec<String> = app
        .mailbox_list()
        .rows
        .iter()
        .map(|row| row.subject.clone())
        .collect();
    for subject in SUBJECTS {
        assert!(
            subjects.iter().any(|held| held == subject),
            "the watched conversation's mail stays in the INBOX unmoved: {subject}"
        );
    }

    // The withdrawal: every copy the proof made leaves, the harness's
    // three-note contract intact.
    proof::delete_subject_mail(&app, &sender, &SUBJECTS).await;
    proof::delete_subject_mail(&app, &recipient, &SUBJECTS).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .all(|row| !SUBJECTS.contains(&row.subject.as_str())),
        "the proof's mail is withdrawn from the recipient's INBOX"
    );
}
