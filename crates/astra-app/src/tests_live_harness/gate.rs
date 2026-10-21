// SPDX-License-Identifier: GPL-3.0-only
//! The gated send-gate proof against the live harness: a scripted veto stands
//! in for a plugin worker, so the gate's fail-closed semantics are proven
//! against real SMTP delivery, not a fake queue. A vetoed send never leaves
//! (the recipient's INBOX never gains it, the queue never holds it), the held
//! row names the plugin and stands Blocked, and a retry after the veto lifts
//! delivers the same composition.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::{BeforeSendGate, GateComposition, GateVerdict, Intent, OutboxIntent};

use super::{harness, inbox_key, proof, row_by_subject};
use crate::FolderRef;

/// The subject the gate proof pins: unique among the harness fixtures.
const SUBJECT: &str = "Harness gate proof";

/// A scripted gate standing in for a plugin worker: veto on, every check
/// answers a block naming its plugin; veto off, an allow.
struct VetoGate {
    veto: AtomicBool,
}

#[async_trait::async_trait]
impl BeforeSendGate for VetoGate {
    async fn check(&self, _composition: &GateComposition) -> GateVerdict {
        if self.veto.load(Ordering::SeqCst) {
            GateVerdict::Block {
                plugin: "dev.astra.veto".to_owned(),
                reason: "the veto is on".to_owned(),
            }
        } else {
            GateVerdict::Allow
        }
    }
}

/// The send gate against the live harness: a vetoed send never leaves, the
/// held row stands Blocked naming the plugin, and the retry after the veto
/// lifts delivers the composition, cleaned up after itself like every
/// delivery proof.
#[tokio::test]
async fn a_vetoed_send_never_leaves_until_the_veto_lifts() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;
    let veto = Arc::new(VetoGate {
        veto: AtomicBool::new(true),
    });
    let app = app.with_send_gate(Some(veto.clone()), None);

    // The same converging sweep the send proof opens with, over this proof's
    // own subject.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The vetoed send: never queued, never delivered, held and named.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: proof::document("Held by the gate, never sent under a veto."),
        blobs: Vec::new(),
    })
    .await
    .unwrap();

    let sending = app.sending();
    assert!(
        sending.rows.is_empty(),
        "a vetoed send is never queued: {:?}",
        sending.rows
    );
    assert_eq!(sending.held.len(), 1, "it stands held");
    let held = &sending.held[0];
    assert_eq!(held.plugin, "dev.astra.veto");
    assert_eq!(held.reason, "the veto is on");

    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .all(|row| row.subject != SUBJECT),
        "nothing reached the recipient under the veto"
    );

    // The veto lifts and the retry re-gates: the same composition delivers.
    veto.veto.store(false, Ordering::SeqCst);
    app.dispatch(Intent::Outbox(OutboxIntent::RetryHeld {
        hold: held.hold.clone(),
    }))
    .await
    .unwrap();

    assert!(
        app.sending().rows.is_empty() && app.sending().held.is_empty(),
        "the retried send delivered and the hold is gone"
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, SUBJECT);
    assert_eq!(
        row.sender_address, "astra1@example.test",
        "the retried send is the sender's own mail"
    );

    // The cleanup: both copies withdrawn, the harness keeping its shape.
    proof::delete_subject_mail(&app, &sender, &[SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .all(|row| row.subject != SUBJECT),
        "the delivered copy is withdrawn from the recipient's INBOX"
    );
}
