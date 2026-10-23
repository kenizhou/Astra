// SPDX-License-Identifier: GPL-3.0-only
//! Reds for recipient resolution: the Thunderbird taxonomy ladder (each
//! state, expiry first, pin-preferred), plus `recipient_trust_failures`.

use super::*;
use crate::keys::{CryptoKeyRecord, CryptoKeyRow};
use crate::store::CryptoStore;

const FAR_FUTURE: u64 = 4_000_000_000; // 2096: any fixture cert reads expired.

fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Insert a public-only S/MIME leaf row bound to `email`; returns its
/// fingerprint.
fn insert_smime_leaf(store: &CryptoStore, email: &str) -> String {
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Resolve CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by(email, &ca);
    insert_smime_der(store, email, &leaf.cert_der, false)
}

fn insert_smime_der(store: &CryptoStore, email: &str, der: &[u8], with_private: bool) -> String {
    let fp = astra_crypto_smime::fingerprint_of_cert_der(der).unwrap();
    crate::upsert_crypto_key(
        store,
        &CryptoKeyRecord {
            row: CryptoKeyRow {
                account_id: "a1".to_string(),
                standard: "smime".to_string(),
                key_type: "cert".to_string(),
                email: Some(email.to_string()),
                fingerprint: fp.clone(),
                origin: "imported".to_string(),
                ..Default::default()
            },
            public_data: hex::encode(der),
            private_data: with_private.then(|| hex::encode(b"soft-private")),
            policy_json: None,
        },
    )
    .unwrap();
    fp
}

#[test]
fn no_key_and_no_collected_hint() {
    let store = test_store();
    let status = resolve_recipient_status(&store, "a1", Standard::Smime, "ghost@x.com");
    assert_eq!(status.state, RecipientTrustState::NoKey);
    assert_eq!(status.fingerprint, None, "no staged key, no hint");
    assert_eq!(status.standard, "smime");
}

#[test]
fn no_key_carries_the_collected_fingerprint_hint() {
    let store = test_store();
    crate::stage_collected_key(
        &store,
        Some("a1"),
        Some("peer@x.com"),
        Some("smime"),
        Some("fpStaged"),
        b"blob",
        Some("wkd"),
    )
    .unwrap();
    let status = resolve_recipient_status(&store, "a1", Standard::Smime, "peer@x.com");
    assert_eq!(status.state, RecipientTrustState::NoKey);
    assert_eq!(
        status.fingerprint.as_deref(),
        Some("fpStaged"),
        "the staged fingerprint rides as the one-click-import hint"
    );
}

#[test]
fn own_key_with_private_material_is_ready() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Own CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("me@x.com", &ca);
    insert_smime_der(&store, "me@x.com", &leaf.cert_der, true);
    let status = resolve_recipient_status(&store, "a1", Standard::Smime, "me@x.com");
    assert_eq!(status.state, RecipientTrustState::Ready);
    assert!(!status.key_expired);
}

#[test]
fn expiry_is_evaluated_before_acceptance() {
    let store = test_store();
    let fp = insert_smime_leaf(&store, "peer@x.com");
    // Even an explicit verified decision cannot revive an expired key.
    crate::put_trust_decision(&store, "a1", "peer@x.com", "smime", &fp, "verified", None).unwrap();
    let status =
        resolve_recipient_status_at(&store, "a1", Standard::Smime, "peer@x.com", FAR_FUTURE);
    assert_eq!(status.state, RecipientTrustState::Expired);
    assert!(status.key_expired);
    assert_eq!(status.fingerprint.as_deref(), Some(fp.as_str()));
}

#[test]
fn decision_ladder_verified_personal_unverified_ready_rejected_blocks() {
    let store = test_store();
    for (decision, expected) in [
        ("verified", RecipientTrustState::Ready),
        ("personal", RecipientTrustState::Ready),
        // Thunderbird model: accepted-without-verification QUALIFIES.
        ("unverified", RecipientTrustState::Ready),
        ("rejected", RecipientTrustState::Rejected),
        ("undecided", RecipientTrustState::Undecided),
    ] {
        let email = format!("{decision}@x.com");
        let fp = insert_smime_leaf(&store, &email);
        crate::put_trust_decision(&store, "a1", &email, "smime", &fp, decision, None).unwrap();
        let status = resolve_recipient_status(&store, "a1", Standard::Smime, &email);
        assert_eq!(
            status.state, expected,
            "decision {decision} resolves to {expected:?}"
        );
        // The raw decision rides along for display except for the explicit
        // "undecided", which falls through to the no-decision arm.
        if decision == "undecided" {
            assert_eq!(status.decision, None);
        } else {
            assert_eq!(status.decision.as_deref(), Some(decision));
        }
    }
}

#[test]
fn no_decision_is_undecided_by_default() {
    let store = test_store();
    insert_smime_leaf(&store, "peer@x.com");
    let status = resolve_recipient_status(&store, "a1", Standard::Smime, "peer@x.com");
    assert_eq!(status.state, RecipientTrustState::Undecided);
    assert!(status.accepted_elsewhere.is_empty());
}

#[test]
fn verified_for_another_address_is_accepted_other_email() {
    let store = test_store();
    // The key is bound to alice-b@x.com (this is the address being resolved);
    // the only verified decision names alice-a@x.com on the same fingerprint.
    let fp = insert_smime_leaf(&store, "alice-b@x.com");
    crate::put_trust_decision(
        &store,
        "a1",
        "alice-a@x.com",
        "smime",
        &fp,
        "verified",
        None,
    )
    .unwrap();
    let status = resolve_recipient_status(&store, "a1", Standard::Smime, "alice-b@x.com");
    assert_eq!(status.state, RecipientTrustState::AcceptedOtherEmail);
    assert_eq!(status.accepted_elsewhere, vec!["alice-a@x.com".to_string()]);
}

#[test]
fn latest_decision_wins_over_a_superseded_one() {
    let store = test_store();
    let fp = insert_smime_leaf(&store, "peer@x.com");
    crate::put_trust_decision(&store, "a1", "peer@x.com", "smime", &fp, "rejected", None).unwrap();
    crate::put_trust_decision(&store, "a1", "peer@x.com", "smime", &fp, "verified", None).unwrap();
    let status = resolve_recipient_status(&store, "a1", Standard::Smime, "peer@x.com");
    assert_eq!(
        status.state,
        RecipientTrustState::Ready,
        "the append-only log's latest row decides"
    );
}

#[test]
fn pin_preferred_handle_picks_the_pinned_candidate() {
    let store = test_store();
    let handles = vec![
        KeyHandleRef {
            handle: astra_crypto_core::KeyHandle::Software(astra_crypto_core::KeyId(
                "smime|fpA".into(),
            )),
            standard: Standard::Smime,
            fingerprint: astra_crypto_core::Fingerprint::new("fpA"),
            usage: astra_crypto_core::KeyUsage::SignAndEncrypt,
            algorithm: "ECDSA-P256".into(),
        },
        KeyHandleRef {
            handle: astra_crypto_core::KeyHandle::Software(astra_crypto_core::KeyId(
                "smime|fpB".into(),
            )),
            standard: Standard::Smime,
            fingerprint: astra_crypto_core::Fingerprint::new("fpB"),
            usage: astra_crypto_core::KeyUsage::SignAndEncrypt,
            algorithm: "ECDSA-P256".into(),
        },
    ];
    // No pin: the first candidate (the deterministic fingerprint order).
    // (`Fingerprint::new` normalises to lowercase.)
    let chosen = pin_preferred_handle(&store, Standard::Smime, "p@x.com", &handles).unwrap();
    assert_eq!(chosen.fingerprint.as_str(), "fpa");
    // A pin to the second candidate wins.
    crate::set_pinned_key(&store, "p@x.com", "smime", "fpB").unwrap();
    let chosen = pin_preferred_handle(&store, Standard::Smime, "p@x.com", &handles).unwrap();
    assert_eq!(chosen.fingerprint.as_str(), "fpb");
    // A pin naming a deleted key falls back to the first candidate.
    crate::set_pinned_key(&store, "p@x.com", "smime", "fpGone").unwrap();
    let chosen = pin_preferred_handle(&store, Standard::Smime, "p@x.com", &handles).unwrap();
    assert_eq!(chosen.fingerprint.as_str(), "fpa");
    // A single candidate short-circuits (no pin lookup).
    let single = &handles[..1];
    let chosen = pin_preferred_handle(&store, Standard::Smime, "p@x.com", single).unwrap();
    assert_eq!(chosen.fingerprint.as_str(), "fpa");
}

#[test]
fn resolution_prefers_the_pinned_key_so_gate_and_encrypt_agree() {
    let store = test_store();
    // Two certs bound to the same email; the pin names the second one. The
    // resolved fingerprint MUST be the pinned one (the interaction fix: a
    // gate passing on one key while encrypting to another produces
    // unreadable ciphertext).
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Multi CA");
    let leaf_a = astra_crypto_smime::testing::build_leaf_signed_by("multi@x.com", &ca);
    let leaf_b = astra_crypto_smime::testing::build_leaf_signed_by("multi@x.com", &ca);
    let fp_a = insert_smime_der(&store, "multi@x.com", &leaf_a.cert_der, false);
    let fp_b = insert_smime_der(&store, "multi@x.com", &leaf_b.cert_der, false);
    let (first, pinned) = if fp_a < fp_b {
        (fp_a, fp_b)
    } else {
        (fp_b, fp_a)
    };
    crate::set_pinned_key(&store, "multi@x.com", "smime", &pinned).unwrap();
    crate::put_trust_decision(
        &store,
        "a1",
        "multi@x.com",
        "smime",
        &pinned,
        "verified",
        None,
    )
    .unwrap();
    // The first-sorted candidate is explicitly rejected: if resolution
    // ignored the pin it would resolve Rejected.
    crate::put_trust_decision(
        &store,
        "a1",
        "multi@x.com",
        "smime",
        &first,
        "rejected",
        None,
    )
    .unwrap();

    let status = resolve_recipient_status(&store, "a1", Standard::Smime, "multi@x.com");
    assert_eq!(status.state, RecipientTrustState::Ready);
    assert_eq!(status.fingerprint.as_deref(), Some(pinned.as_str()));
}

#[test]
fn trust_failures_lists_every_non_ready_recipient() {
    let store = test_store();
    // me@x.com: own key, Ready. peer@x.com: verified, Ready. ghost@x.com:
    // NoKey. blocked@x.com: rejected.
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Gate CA");
    let own = astra_crypto_smime::testing::build_leaf_signed_by("me@x.com", &ca);
    insert_smime_der(&store, "me@x.com", &own.cert_der, true);
    let fp_peer = insert_smime_leaf(&store, "peer@x.com");
    crate::put_trust_decision(
        &store,
        "a1",
        "peer@x.com",
        "smime",
        &fp_peer,
        "verified",
        None,
    )
    .unwrap();
    let fp_blocked = insert_smime_leaf(&store, "blocked@x.com");
    crate::put_trust_decision(
        &store,
        "a1",
        "blocked@x.com",
        "smime",
        &fp_blocked,
        "rejected",
        None,
    )
    .unwrap();

    let failures = recipient_trust_failures(
        &store,
        "a1",
        Standard::Smime,
        &[
            "me@x.com".to_string(),
            "peer@x.com".to_string(),
            "ghost@x.com".to_string(),
            "blocked@x.com".to_string(),
        ],
    );
    assert_eq!(failures.len(), 2);
    assert_eq!(failures[0].email, "ghost@x.com");
    assert_eq!(failures[0].state, RecipientTrustState::NoKey);
    assert_eq!(failures[1].email, "blocked@x.com");
    assert_eq!(failures[1].state, RecipientTrustState::Rejected);
    assert_eq!(
        failures[1].fingerprint.as_deref(),
        Some(fp_blocked.as_str())
    );
}

#[test]
fn key_material_expired_reads_smime_not_after_and_tolerates_garbage() {
    let store = test_store();
    let _ = &store; // the helper is pure; the store is unused here.
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Expiry CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("exp@x.com", &ca);
    let hex_blob = hex::encode(&leaf.cert_der);
    assert!(!key_material_expired(
        Standard::Smime,
        &hex_blob,
        now_unix()
    ));
    assert!(key_material_expired(Standard::Smime, &hex_blob, FAR_FUTURE));
    // Parse failures read as NOT expired: the trust gate must not invent an
    // expiry for a corrupt blob (it fails later with a precise error).
    assert!(!key_material_expired(Standard::Smime, "zzzz", now_unix()));
    assert!(!key_material_expired(Standard::OpenPgp, "zzzz", now_unix()));
}

#[test]
fn key_material_expired_openpgp_fresh_cert_is_not_expired() {
    let cert = astra_crypto_openpgp::engine::generate("Fresh <fresh@x.com>").unwrap();
    let blob = astra_crypto_openpgp::keymap::cert_to_public_blob(&cert).unwrap();
    assert!(!key_material_expired(
        Standard::OpenPgp,
        &hex::encode(blob),
        now_unix()
    ));
}
