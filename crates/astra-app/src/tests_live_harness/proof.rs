// SPDX-License-Identifier: GPL-3.0-only
//! The fixtures the live proof suites share (send, reply, forward): the
//! smallest document a send holds, the two-account app both sides of a
//! delivery are read through, and the outbox-mediated cleanup sweep that
//! withdraws a proof's copies so the harness keeps its seeded shape.
//!
//! The cleanup is the contract the proofs owe the suites that pin the
//! harness: a proof's mail leaves every folder it reached (the recipient's
//! INBOX, the sender's Sent) through the engine's own delete path, so
//! astra1's INBOX is the three seeded notes again and astra2 holds nothing.
//! The sweep also runs at the start of each proof, so a run interrupted
//! before its cleanup still converges.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use astra_viewmodel::FolderRole;
use engine_api::{AccountId, EmailAddress, Engine, MailEdit, Provider};

use crate::{Account, App, AppObserver};

use super::{dial_with_accepted_certificate, CountingObserver};

/// The receiving account's address, the second harness user.
pub(super) const RECIPIENT: &str = "astra2@example.test";

/// A one-paragraph document carrying `text`, the smallest document a send holds.
pub(super) fn document(text: &str) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![paragraph(text)],
        attachments: Vec::new(),
    }
}

/// A one-paragraph block carrying `text`: the composer's typed words above a
/// retained quote block, the shape a reply or forward sends.
pub(super) fn paragraph(text: &str) -> Block {
    Block::Paragraph(Paragraph {
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
    })
}

/// A unique idempotency key for one cleanup delete, from the wall clock and a
/// process counter: unique per call, so deleting several copies never collapses
/// into one op.
pub(super) fn cleanup_idempotency() -> String {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("harness-proof-cleanup:{nanos}:{seq}")
}

/// The account's folder key carrying `role`, as the pane projects it: the
/// fixture for the Sent folder, where the dial binds the folder the engine
/// files the sender's copy into.
pub(super) fn role_key(
    app: &App<Box<dyn Provider>>,
    account: &AccountId,
    role: FolderRole,
) -> String {
    app.mailbox_list()
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .and_then(|row| row.folders.iter().find(|folder| folder.role == Some(role)))
        .map_or_else(
            || panic!("the pane lists the account's {role:?} folder"),
            |folder| folder.key.as_str().to_owned(),
        )
}

/// Deletes every message carrying one of `subjects` from `account` through the
/// engine's own outbox-mediated delete, the write path a delete action rides:
/// queued durably first, then applied to the server. A message is addressed by
/// its own key, so one folder's provider can withdraw another folder's copy
/// (an INBOX delivery and its Sent filing alike).
pub(super) async fn delete_subject_mail(
    app: &App<Box<dyn Provider>>,
    account: &AccountId,
    subjects: &[&str],
) {
    let handle = app
        .account_handle(account)
        .await
        .expect("the account the test dialed");
    let provider = handle
        .providers
        .first()
        .expect("the account's provider to delete through");
    let targets: Vec<_> = app
        .engine
        .messages(account)
        .await
        .expect("the account's stored messages")
        .into_iter()
        .filter(|message| {
            message
                .envelope
                .subject
                .as_deref()
                .is_some_and(|subject| subjects.contains(&subject))
        })
        .map(|message| message.id.key().clone())
        .collect();
    for target in targets {
        app.engine
            .edit_mail(
                provider,
                account,
                &cleanup_idempotency(),
                &MailEdit::Delete { target },
            )
            .await
            .expect("the cleanup delete");
    }
}

/// Dials both harness accounts through the product's own certificate
/// acceptance walk and returns the app over them (astra1 first, astra2
/// second), with both account ids: the shape every proof that needs both
/// sides of a delivery builds, the same way the desktop host builds its loop.
pub(super) async fn both_accounts_app(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (App<Box<dyn Provider>>, AccountId, AccountId) {
    let (sender, sender_providers, ..) =
        dial_with_accepted_certificate(host, port, user, password).await;
    let (recipient, recipient_providers, ..) =
        dial_with_accepted_certificate(host, port, RECIPIENT, password).await;
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![
            Account {
                id: sender.clone(),
                providers: sender_providers,
                identity: EmailAddress::new("astra1@example.test"),
            },
            Account {
                id: recipient.clone(),
                providers: recipient_providers,
                identity: EmailAddress::new(RECIPIENT),
            },
        ],
        None,
        observer,
    );
    (app, sender, recipient)
}
