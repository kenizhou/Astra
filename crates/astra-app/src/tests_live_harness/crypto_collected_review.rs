// SPDX-License-Identifier: GPL-3.0-only
//! The gated collected-key review proof (SP4d-2): the promote-plus-trust pairing
//! end to end through the product surface. astra2 receives an Autocrypt mail
//! from astra1 (the raw submission seeds the wire; the consume runs through the
//! product's reading pipeline), lists the staged key through `App::list_collected`,
//! and promotes it through `App::promote_collected`. The promote records the
//! unverified acceptance, so astra2 then sends astra1 encrypted mail admitted by
//! the promote ALONE (no programmatic trust seeding): the gate resolves the
//! recipient Ready, the engine's outbox delivers it sealed, and astra1 opens it
//! to the words and the verified banner. A gossip mail stages a second peer key
//! (astra3), which astra2 rejects through `App::reject_collected`; the staging
//! row is gone and the rejected decision stands. Every copy is withdrawn.
//!
//! The dials mirror the receive and import proofs (one accepted self-signed
//! certificate for IMAP and SMTP): astra2's is wrapped (it sends), astra1's
//! ordinary (it receives the proof send). The seeding helpers ride the child
//! `helpers` module so this file stays under the line cap.

use std::sync::Arc;

use astra_crypto_core::{CryptoMethod, RecipientTrustState, Standard};
use astra_crypto_store::{
    autocrypt, crypto_policy, get_latest_trust_decision, put_trust_decision,
    resolve_recipient_status, CryptoStore,
};
use engine_api::{EmailAddress, Engine};

use crate::crypto::CryptoFlags;
use crate::crypto_decode::CryptoDecoder;
use crate::{Account, App, AppObserver, FolderRef, Intent};

use super::{
    account as harness_config,
    crypto_send::dial_wrapped,
    dial_with_accepted_certificate, harness, inbox_key,
    proof::{self, document},
    row_by_subject, CountingObserver,
};

#[path = "crypto_collected_review/helpers.rs"]
mod helpers;
use helpers::{
    gate_source, import_contact, minimal_blob, open_current, public_blob, seed_own, submit_raw,
    SENDER, SUBJECT, WORDS,
};

/// A third peer the gossip arm stages: a store-only key, not a harness user.
const ASTRA3: &str = "astra3@example.test";

/// The gated run: the consume stages a peer key, the review surface lists and
/// promotes it (the trust decision lands), the promote alone admits an encrypted
/// send the peer decrypts, and a second staged key is rejected and removed.
#[tokio::test]
async fn the_collected_review_promotes_and_admits_the_send_through_the_product_surface() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    // The fixed test master key keeps the proof off the OS keyring.
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);

    // The store first (the boot's ordering). astra2 holds its own decrypt key but
    // NO keystore cert for astra1: the promote lands it. astra1 holds astra2's and
    // astra3's certs (verified) so the gate admits the staging mails.
    let sender_id = harness_config(&host, &port, &user, &password, Vec::new())
        .account_id()
        .expect("the sender's account id");
    let recipient_id = harness_config(&host, &port, proof::RECIPIENT, &password, Vec::new())
        .account_id()
        .expect("the recipient's account id");
    let store = Arc::new(CryptoStore::open_in_memory().expect("the in-memory store opens"));
    store.register_account(sender_id.as_str(), SENDER).unwrap();
    store
        .register_account(recipient_id.as_str(), proof::RECIPIENT)
        .unwrap();
    store.register_account("astra3", ASTRA3).unwrap();
    let sender_fp = seed_own(&store, sender_id.as_str(), SENDER).await;
    let recipient_fp = seed_own(&store, recipient_id.as_str(), proof::RECIPIENT).await;
    let astra3_fp = seed_own(&store, "astra3", ASTRA3).await;

    crypto_policy::set_policy_flag(&store, sender_id.as_str(), "send_autocrypt_headers", true)
        .unwrap();
    let recipient_blob = public_blob(&store, recipient_id.as_str(), &recipient_fp);
    import_contact(
        &store,
        sender_id.as_str(),
        proof::RECIPIENT,
        &recipient_blob,
        &recipient_fp,
    );
    put_trust_decision(
        &store,
        sender_id.as_str(),
        proof::RECIPIENT,
        "openpgp",
        &recipient_fp,
        "verified",
        None,
    )
    .unwrap();
    let astra3_blob = public_blob(&store, "astra3", &astra3_fp);
    import_contact(&store, sender_id.as_str(), ASTRA3, &astra3_blob, &astra3_fp);
    put_trust_decision(
        &store,
        sender_id.as_str(),
        ASTRA3,
        "openpgp",
        &astra3_fp,
        "verified",
        None,
    )
    .unwrap();
    let minimal = minimal_blob(&store, sender_id.as_str(), &sender_fp);

    // astra2's dial is wrapped (it sends); astra1's ordinary (it receives). The
    // decoder rides the shared store; the review surface reads the same store.
    let wrapper = astra_crypto_store::mime_wrapper_for(
        &store,
        recipient_id.as_str(),
        proof::RECIPIENT,
        CryptoMethod::Openpgp,
    )
    .expect("a crypto method installs a wrapper");
    let (recipient, recipient_providers) =
        dial_wrapped(&host, &port, proof::RECIPIENT, &password, wrapper).await;
    assert_eq!(
        recipient, recipient_id,
        "the dial answers the config-derived id"
    );
    let (sender, sender_providers, _, _) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;
    assert_eq!(sender, sender_id);
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![
            Account {
                id: sender.clone(),
                providers: sender_providers,
                identity: EmailAddress::new(SENDER),
            },
            Account {
                id: recipient.clone(),
                providers: recipient_providers,
                identity: EmailAddress::new(proof::RECIPIENT),
            },
        ],
        None,
        observer,
    )
    .with_decoder(Some(Arc::new(CryptoDecoder::new(store.clone()))));
    app.install_crypto_store(Arc::clone(&store));

    // A known state: sweep any earlier run's proof mail first.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[SUBJECT, "..."]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT, "..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The staging: a cleartext Autocrypt header whose addr matches From carries
    // astra1's key; the product's own open runs the consume that stages it.
    let source = gate_source(&store, sender_id.as_str(), &sender_fp, &[proof::RECIPIENT]).await;
    let header = autocrypt::emit_autocrypt_header(SENDER, &minimal);
    submit_raw(
        &host,
        &password,
        &autocrypt::inject_headers(&source, &[header]),
    );
    let reading = open_current(&app, &recipient).await;
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "the staging mail decrypts: {:?}",
        reading.plain
    );

    // The review list, through the product surface: the staged key answers.
    let staged = app
        .list_collected(&recipient)
        .expect("the review list answers");
    let staged_row = staged
        .iter()
        .find(|row| row.fingerprint.as_deref() == Some(sender_fp.as_str()))
        .expect("the sender key stages");
    assert_eq!(staged_row.source.as_deref(), Some("autocrypt"));
    assert_eq!(staged_row.peer_email.as_deref(), Some(SENDER));

    // The promote, through the product surface: the key lands AND the trust
    // decision lands with it (the load-bearing pairing). No programmatic seeding.
    let promoted = app
        .promote_collected(
            &recipient,
            Standard::OpenPgp,
            &sender_fp,
            Some(SENDER),
            false,
        )
        .expect("the promote lands");
    assert_eq!(promoted.fingerprint, sender_fp);
    assert_eq!(promoted.key_type, "public");
    assert_eq!(promoted.origin, "autocrypt", "the provenance survives");
    assert!(
        !promoted.has_private,
        "a collected promote never flags a private key"
    );
    let decision =
        get_latest_trust_decision(&store, recipient.as_str(), SENDER, "openpgp", &sender_fp)
            .expect("the trust read runs")
            .expect("the promote recorded a decision");
    assert_eq!(decision.decision, "unverified");
    let status = resolve_recipient_status(&store, recipient.as_str(), Standard::OpenPgp, SENDER);
    assert_eq!(
        status.state,
        RecipientTrustState::Ready,
        "the promoted key is send-usable by the promote alone"
    );
    assert!(
        !app.list_collected(&recipient)
            .unwrap()
            .iter()
            .any(|row| row.fingerprint.as_deref() == Some(sender_fp.as_str())),
        "the promote drains the staging row"
    );

    // Withdraw the staging mail so the gossip arm's placeholder stays unambiguous,
    // then send astra1 encrypted mail admitted by the promote.
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::SubmitRichMail {
        from: recipient.clone(),
        to: SENDER.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: document(WORDS),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
        composition: None,
    })
    .await
    .unwrap();
    assert!(
        app.sending().rows.is_empty(),
        "the promote-admitted crypto send leaves nothing in the queue: {:?}",
        app.sending()
    );

    // The delivery: astra1 opens the sealed mail through the product, its own key
    // decrypting and astra2's cert (a contact) verifying the signature.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let sender_inbox =
        FolderRef::from_parts(sender.as_str(), inbox_key(&app, &sender)).expect("the INBOX");
    app.dispatch(Intent::SelectFolder {
        folder: sender_inbox,
    })
    .await
    .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, "...");
    let astra_viewmodel::RowKey::Message { key, .. } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::OpenMessage {
        message: crate::reference::MessageRef {
            account: sender.clone(),
            key: key.clone(),
        },
    })
    .await
    .unwrap();
    let delivered = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert_eq!(delivered.subject, SUBJECT, "the protected subject restores");
    assert!(
        delivered
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "the composer's words render: {:?}",
        delivered.plain
    );
    let banner = delivered
        .security_banner
        .as_ref()
        .expect("the verified banner rides the snapshot");
    assert!(
        banner.text.contains("signature verified"),
        "the banner states the verified signature: {}",
        banner.text
    );
    let result =
        astra_crypto_store::get_message_crypto_result(&store, sender.as_str(), key.as_str())
            .expect("the result row lands");
    assert_eq!(result.decrypt_state, "ok");
    assert_eq!(result.signature_state, "valid-verified");
    assert_eq!(
        result.signer_fingerprint.as_deref(),
        Some(recipient_fp.as_str())
    );

    // The reject arm: a gossip mail (encrypted to two non-sender recipients)
    // stages astra3's peer key; the review surface rejects it, the row is gone.
    let source = gate_source(
        &store,
        sender_id.as_str(),
        &sender_fp,
        &[proof::RECIPIENT, ASTRA3],
    )
    .await;
    submit_raw(&host, &password, &source);
    let reading = open_current(&app, &recipient).await;
    assert!(reading
        .plain
        .as_deref()
        .is_some_and(|body| body.contains(WORDS)));
    let staged = app
        .list_collected(&recipient)
        .expect("the review list answers");
    let astra3_row = staged
        .iter()
        .find(|row| row.peer_email.as_deref() == Some(ASTRA3))
        .expect("the gossip peer key stages");
    assert_eq!(astra3_row.fingerprint.as_deref(), Some(astra3_fp.as_str()));
    app.reject_collected(
        &recipient,
        astra3_row.id,
        Standard::OpenPgp,
        &astra3_fp,
        Some(ASTRA3),
    )
    .expect("the reject lands");
    assert!(
        !app.list_collected(&recipient)
            .unwrap()
            .iter()
            .any(|row| row.peer_email.as_deref() == Some(ASTRA3)),
        "the reject removes the staging row"
    );
    let rejected =
        get_latest_trust_decision(&store, recipient.as_str(), ASTRA3, "openpgp", &astra3_fp)
            .expect("the trust read runs")
            .expect("the reject recorded a decision");
    assert_eq!(rejected.decision, "rejected");

    // The cleanup: every proof mail leaves both accounts through the engine's
    // own delete path (the INBOX and the Sent alike).
    proof::delete_subject_mail(&app, &sender, &["..."]).await;
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
