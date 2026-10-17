// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the attachment byte read: the one part the reading snapshot's
//! row names, answered with the row's file name beside the decoded bytes,
//! the no-match answer for an id the message carries no part for, the
//! unresolved answer for a message the store does not hold, and the
//! engine's own sentence when the fetch refuses. The fixtures live in
//! `dispatch_fakes`, the crate-root test module every dispatch-shaped test
//! shares.

use std::sync::{Arc, Mutex};

use engine_api::{AccountId, ProviderKey};

use crate::{dispatch_fakes as fakes, AttachmentContent, AttachmentReadError, Intent, Surface};

/// One message carrying a single pdf attachment: base64 `UERG`, the three
/// bytes `PDF`.
fn source() -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(
        b"Content-Type: multipart/mixed; boundary=\"mix\"\r\n\r\n\
          --mix\r\nContent-Type: text/plain; charset=\"utf-8\"\r\n\r\n\
          The launch is ready.\r\n",
    );
    raw.extend_from_slice(
        b"--mix\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\n\
          Content-Disposition: attachment; filename=\"report.pdf\"\r\n\
          Content-Transfer-Encoding: base64\r\n\r\nUERG\r\n",
    );
    raw.extend_from_slice(b"--mix--\r\n");
    raw
}

/// An app over one account whose Inbox holds the attached message `m1`,
/// synced, so the read has a store and a source to decode.
async fn attached_mailbox(surfaces: &Arc<Mutex<Vec<Surface>>>) -> crate::App<fakes::FakeProvider> {
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::delivered_at(
            fakes::mail("m1", "inbox", "Launch"),
            8,
            0,
        )],
        vec![("m1".to_owned(), source())],
    );
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// The account and provider key every read fixture names.
fn parts() -> (AccountId, ProviderKey) {
    (
        AccountId::try_from("work").expect("the account id"),
        ProviderKey::new("m1").expect("the provider key"),
    )
}

#[tokio::test]
async fn the_read_answers_the_rows_file_name_and_the_decoded_bytes() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = attached_mailbox(&surfaces).await;
    let (account, key) = parts();

    let content = app
        .attachment_content(&account, &key, 0)
        .await
        .expect("the read answers")
        .expect("the part matches");
    assert_eq!(
        content,
        AttachmentContent {
            file_name: "report.pdf".to_owned(),
            bytes: b"PDF".to_vec(),
        },
        "the row's own file name beside the transfer-decoded bytes",
    );
}

#[tokio::test]
async fn the_read_answers_none_when_no_part_matches_the_id() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = attached_mailbox(&surfaces).await;
    let (account, key) = parts();

    let none = app
        .attachment_content(&account, &key, 7)
        .await
        .expect("the read answers");
    assert_eq!(none, None, "an id the message carries no part for");
}

#[tokio::test]
async fn the_read_is_unresolved_for_a_message_the_store_does_not_hold() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = attached_mailbox(&surfaces).await;
    let account = AccountId::try_from("work").expect("the account id");
    let missing = ProviderKey::new("m-none").expect("the provider key");

    let refused = app
        .attachment_content(&account, &missing, 0)
        .await
        .expect_err("an unresolved message refuses");
    assert_eq!(
        refused,
        AttachmentReadError::Unresolved,
        "the refusal is typed, not a sentence to parse"
    );
}

#[tokio::test]
async fn the_read_carries_the_engines_sentence_when_the_fetch_refuses() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // The message is in the store but carries no raw source, so the engine's
    // cache-first fetch refuses at the provider: the sentence the host's
    // save failure is answered with.
    let provider = fakes::FakeProvider::with_mail(
        vec![fakes::inbox()],
        vec![fakes::delivered_at(
            fakes::mail("m1", "inbox", "Launch"),
            8,
            0,
        )],
    );
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let (account, key) = parts();

    let refused = app
        .attachment_content(&account, &key, 0)
        .await
        .expect_err("the refused fetch surfaces");
    let AttachmentReadError::Fetch(sentence) = refused else {
        panic!("the refusal is the fetch, not the resolution: {refused:?}");
    };
    assert!(
        sentence.contains("the fake holds no source for that key"),
        "the engine's own sentence rides the refusal: {sentence}"
    );
}
