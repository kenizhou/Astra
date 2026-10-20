// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the forward's carried attachments: the wire stays additive over
//! `carry` (a bare forward crosses exactly as it always did), the carried
//! parts pre-attach onto the published document as removable rows, and an
//! id the original carries no part for pre-attaches nothing.

use std::sync::{Arc, Mutex};

use astra_composer::AttachmentDisposition;

use crate::{
    compose_request::carry::{handle_of, standing},
    dispatch_fakes as fakes, ComposeIntent, Intent, MessageRef, Surface,
};

/// The original's raw source: a plain body and one pdf attachment, base64
/// `UERG`, the three bytes `PDF`, the part the reading snapshot lists as id 0.
fn attached_source() -> Vec<u8> {
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

/// An app over one `work` account whose Inbox holds the attached original
/// `m1`, synced, so the compose arm and the submit both have a store and a
/// source to read.
async fn app_over_attached(surfaces: &Arc<Mutex<Vec<Surface>>>) -> crate::App<fakes::FakeProvider> {
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::delivered_at(
            fakes::mail("m1", "inbox", "Launch"),
            8,
            0,
        )],
        vec![("m1".to_owned(), attached_source())],
    );
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// The reference to the original, the message every fixture answers.
fn m1() -> MessageRef {
    MessageRef::from_parts("work", "m1".to_owned()).unwrap()
}

#[test]
fn the_forward_wire_stays_additive_over_the_carry() {
    let bare: ComposeIntent =
        serde_json::from_str(r#"{"forward":{"message":{"account":"work","key":"m1"}}}"#)
            .expect("the bare forward deserialises");
    assert_eq!(
        bare,
        ComposeIntent::Forward {
            message: m1(),
            carry: Vec::new(),
        },
        "a forward without carry crosses as the empty one, the old shape"
    );
    let carried: ComposeIntent = serde_json::from_str(
        r#"{"forward":{"message":{"account":"work","key":"m1"},"carry":[0,2]}}"#,
    )
    .expect("the carried forward deserialises");
    assert_eq!(
        carried,
        ComposeIntent::Forward {
            message: m1(),
            carry: vec![0, 2],
        }
    );
    let empty = serde_json::to_string(&bare).unwrap();
    assert!(
        !empty.contains("carry"),
        "an empty carry serialises as the bare forward: {empty}"
    );
}

#[tokio::test]
async fn a_forward_prefills_the_carried_parts_as_removable_rows() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over_attached(&surfaces).await;

    app.dispatch(Intent::Compose(ComposeIntent::Forward {
        message: m1(),
        carry: vec![0],
    }))
    .await
    .unwrap();

    let snapshot = app.compose_request().expect("the forward's prefill");
    let [row] = snapshot.document.attachments.as_slice() else {
        panic!(
            "the carried part stands as one removable row, not {:?}",
            snapshot.document.attachments
        );
    };
    assert_eq!(row.file_name, "report.pdf");
    assert_eq!(row.media_type, "application/pdf");
    assert_eq!(row.size, Some(3));
    assert_eq!(row.disposition, AttachmentDisposition::Attachment);
    assert_eq!(
        row.blob
            .as_ref()
            .map(astra_composer::DraftBlobHandle::as_str),
        Some(handle_of(0).as_str()),
        "the row's handle names the original's part for the submit to resolve"
    );
    assert_eq!(row.data_url, None);
}

#[tokio::test]
async fn a_carry_naming_no_part_pre_attaches_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over_attached(&surfaces).await;

    app.dispatch(Intent::Compose(ComposeIntent::Forward {
        message: m1(),
        carry: vec![7],
    }))
    .await
    .unwrap();

    let snapshot = app.compose_request().expect("the forward's prefill");
    assert!(
        snapshot.document.attachments.is_empty(),
        "no row for a part the original does not carry"
    );
}

#[test]
fn the_standing_carry_drops_ids_whose_rows_were_removed() {
    let document = astra_composer::ComposerDocument {
        blocks: Vec::new(),
        attachments: vec![astra_composer::DraftAttachment {
            id: astra_composer::AttachmentId::new("carry-0").unwrap(),
            blob: Some(astra_composer::DraftBlobHandle::new(handle_of(0)).unwrap()),
            file_name: "report.pdf".to_owned(),
            media_type: "application/pdf".to_owned(),
            size: Some(3),
            disposition: AttachmentDisposition::Attachment,
            data_url: None,
        }],
    };
    assert_eq!(
        standing(&[0, 2], &document),
        vec![0],
        "only the id whose row still stands rides"
    );
    assert_eq!(
        standing(&[2], &document),
        Vec::<u32>::new(),
        "a removed row carries nothing"
    );
}
