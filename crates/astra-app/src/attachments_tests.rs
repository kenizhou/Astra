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

use crate::{
    dispatch_fakes as fakes, AttachmentContent, AttachmentReadError, EmlContent, EmlReadError,
    Intent, Surface,
};

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

/// The raw source one message arrives as: a folded subject header, a
/// non-canonical header order and a quoted-printable body, the details a
/// re-serialization of the projection would tidy away and an export keeps
/// verbatim.
fn eml_source() -> Vec<u8> {
    b"Subject: Re:\r\n quarterly report\r\n\
      Content-Transfer-Encoding: quoted-printable\r\n\
      Content-Type: text/plain; charset=\"utf-8\"\r\n\
      From: Someone <someone@example.com>\r\n\r\n\
      caf=C3=A9\r\n"
        .to_vec()
}

/// An app over one account whose Inbox holds the sourced message `m2`,
/// synced, so the `.eml` read has a store and a source to answer with.
async fn sourced_mailbox(surfaces: &Arc<Mutex<Vec<Surface>>>) -> crate::App<fakes::FakeProvider> {
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::delivered_at(
            fakes::mail("m2", "inbox", "Re: quarterly report"),
            8,
            30,
        )],
        vec![("m2".to_owned(), eml_source())],
    );
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// The account and provider key the `.eml` fixtures name.
fn eml_parts() -> (AccountId, ProviderKey) {
    (
        AccountId::try_from("work").expect("the account id"),
        ProviderKey::new("m2").expect("the provider key"),
    )
}

#[tokio::test]
async fn the_eml_read_answers_the_subjects_suggested_name_and_the_source_verbatim() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = sourced_mailbox(&surfaces).await;
    let (account, key) = eml_parts();

    let content = app
        .eml_content(&account, &key)
        .await
        .expect("the read answers");

    assert_eq!(
        content,
        EmlContent {
            file_name: "Re quarterly report.eml".to_owned(),
            bytes: eml_source(),
        },
        "the subject's sanitised name beside the source bytes, verbatim: no \
         re-serialization folds the header back, reorders it, or decodes the body",
    );
}

#[tokio::test]
async fn the_eml_read_is_unresolved_for_a_message_the_store_does_not_hold() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = sourced_mailbox(&surfaces).await;
    let account = AccountId::try_from("work").expect("the account id");
    let missing = ProviderKey::new("m-none").expect("the provider key");

    let refused = app
        .eml_content(&account, &missing)
        .await
        .expect_err("an unresolved message refuses");
    assert_eq!(
        refused,
        EmlReadError::Unresolved,
        "the refusal is typed, not a sentence to parse"
    );
}

#[tokio::test]
async fn the_eml_read_carries_the_engines_sentence_when_the_fetch_refuses() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // The message is in the store but carries no raw source, so the engine's
    // cache-first fetch refuses at the provider: the sentence the host's
    // save failure is answered with.
    let provider = fakes::FakeProvider::with_mail(
        vec![fakes::inbox()],
        vec![fakes::delivered_at(
            fakes::mail("m2", "inbox", "Re: quarterly report"),
            8,
            30,
        )],
    );
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let (account, key) = eml_parts();

    let refused = app
        .eml_content(&account, &key)
        .await
        .expect_err("the refused fetch surfaces");
    let EmlReadError::Fetch(sentence) = refused else {
        panic!("the refusal is the fetch, not the resolution: {refused:?}");
    };
    assert!(
        sentence.contains("the fake holds no source for that key"),
        "the engine's own sentence rides the refusal: {sentence}"
    );
}

#[test]
fn the_suggested_name_keeps_a_readable_subject_and_adds_the_suffix() {
    assert_eq!(
        super::suggested_eml_name("Quarterly report"),
        "Quarterly report.eml",
        "a plain subject crosses with only the suffix added"
    );
    assert_eq!(
        super::suggested_eml_name("Re: quarterly (the 2026 edition)"),
        "Re quarterly (the 2026 edition).eml",
        "the readable punctuation a file name carries stays"
    );
}

#[test]
fn the_suggested_name_sanitises_what_a_file_name_cannot_carry() {
    assert_eq!(
        super::suggested_eml_name("Re: quarterly/report?"),
        "Re quarterly report.eml",
        "path separators and Windows' forbidden set fold to spaces, runs collapse"
    );
    assert_eq!(
        super::suggested_eml_name("a\u{7f}b\u{0}c"),
        "a b c.eml",
        "control characters fold the same way"
    );
    assert_eq!(
        super::suggested_eml_name("ends in dots..."),
        "ends in dots.eml",
        "trailing dots shed before the suffix lands"
    );
}

#[test]
fn the_suggested_name_falls_back_when_the_subject_sanitises_to_nothing() {
    assert_eq!(
        super::suggested_eml_name(""),
        "message.eml",
        "no subject at all suggests the plain name"
    );
    assert_eq!(
        super::suggested_eml_name("???"),
        "message.eml",
        "a subject that sanitises to nothing suggests the same"
    );
}

#[test]
fn the_suggested_name_falls_back_for_a_windows_reserved_device_name() {
    assert_eq!(
        super::suggested_eml_name("con"),
        "message.eml",
        "the reserved device name the write would refuse falls back"
    );
    assert_eq!(
        super::suggested_eml_name("Com1 status"),
        "Com1 status.eml",
        "a stem that only begins with one keeps its name"
    );
}

#[test]
fn the_suggested_name_clamps_a_long_subject_before_the_suffix() {
    let long = "q".repeat(300);
    let name = super::suggested_eml_name(&long);
    assert_eq!(
        name.chars().count(),
        80 + ".eml".len(),
        "the stem clamps at the cap and the suffix rides whole: {name}"
    );
}
