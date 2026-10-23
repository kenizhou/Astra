// SPDX-License-Identifier: GPL-3.0-only
//! The built-in crypto store: kylins' `store::crypto` layer and
//! `platform::keystore` bridge ported to Astra's store idiom (rusqlite with
//! an embedded migration chain, the same stack `astra-ai` rides). One
//! `crypto.sqlite` beside the AI store holds the key material, the
//! append-only trust-decision audit, the staged (collected) keys, the
//! per-contact key pins, the CRL cache, and the per-message crypto results.
//!
//! The store is synchronous (rusqlite under a `Mutex`, like `astra-ai`'s
//! store); the [`KeyStore`] trait it implements is async, and the bridge
//! methods simply run the sync queries inside the async signature.
//!
//! Porting notes (deviations from kylins, each recorded where it lives):
//! - sqlx becomes rusqlite; `Result<T, String>` stays the store-layer error
//!   convention so the ported query bodies stay verbatim.
//! - The schema carries a minimal `accounts` table (id, email) because
//!   `crypto_keys` references it and the bridge's `put` reads the account
//!   email from it; the app registers an account row when it configures a
//!   mail account.
//! - Private-key wrapping binds `astra:{account}:private_key:1` as the AAD
//!   (kylins used the `kylins:` prefix; a fresh store, so no compatibility
//!   concern), under the keyring entry `astra-mail` / `crypto-master-key`.

pub mod autocrypt;
mod certs;
mod collected;
mod crl;
pub mod crypto_policy;
mod default;
mod keys;
mod keystore;
mod outbox;
mod password;
mod pinned;
mod policy;
mod resolve;
mod store;
pub mod transport;
mod trust;
pub mod vault;

pub use certs::{
    get_crypto_key_by_token, insert_harvested_cert, list_intermediate_certs,
    list_trust_anchor_certs, upsert_intermediate_cert,
};
pub use collected::{
    clear_collected_keys, list_collected_keys_for_account, list_collected_keys_for_fingerprint,
    list_collected_keys_for_peer, promote_collected_key, remove_collected_key, stage_collected_key,
    stage_collected_key_if_absent, CollectedKeyRow,
};
pub use crl::{get_crl, prune_stale_crls, upsert_crl, CrlCacheRow};
pub use default::{
    backfill_cert_summaries, delete_crypto_key, get_default_signing_key, set_default_signing_key,
    DefaultKeyRow,
};
pub use keys::{
    get_crypto_key_full, get_crypto_key_public, get_crypto_key_public_with_blob,
    list_crypto_keys_for_account, list_crypto_keys_for_email, upsert_crypto_key, CryptoKeyRecord,
    CryptoKeyRow,
};
pub use keystore::SqliteKeyStore;
pub use outbox::{
    enqueue_crypto_send, get_crypto_outbox_row, list_crypto_outbox, list_due_crypto_sends,
    mark_crypto_failed, mark_crypto_sending, mark_crypto_unconfirmed, requeue_crypto_send,
    settle_crypto_send, CryptoOutboxRow,
};
pub use password::apply_password_crypto;
pub use pinned::{list_pinned_keys, remove_pinned_key, set_pinned_key, PinnedKey};
pub use policy::StoreCryptoPolicy;
pub use resolve::{
    key_material_expired, pin_preferred_handle, recipient_trust_failures, resolve_recipient_status,
    resolve_recipient_status_at, RecipientTrustFailure, RecipientTrustState, RecipientTrustStatus,
};
pub use store::CryptoStore;
pub use transport::{
    file_sent_copy, submit_smtp, ImapDial, SmtpDial, SubmitFailure, SubmitFailureKind,
    TransportSecurity,
};
pub use trust::{
    get_latest_trust_decision, list_decisions_for_email, list_latest_decisions_for_key,
    put_trust_decision, TrustDecisionRow,
};
