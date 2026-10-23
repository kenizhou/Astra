// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the crypto halves of the host's account store and resolver:
//! the method write persists and echoes, and the resolver answers the
//! method and dial table per account (the typed unavailable answer when a
//! method stands without both dial halves).

use std::sync::Arc;

use engine_api::AccountId;

use astra_app::{CryptoTransportResolver, HostAccountOps};

use super::HostCryptoResolver;
use crate::account_ops::HostAccountStore;
use crate::commands_fakes::{scratch, FakeStore};
use crate::credential_store::AccountCredentialStore;
use crate::dial_table::AccountDials;

/// A full IMAP+SMTP config with no crypto section.
fn mail_config_toml() -> &'static str {
    concat!(
        "id = \"work\"\n",
        "[imap]\n",
        "addr = \"imap.example.net:993\"\n",
        "server_name = \"imap.example.net\"\n",
        "username = \"ada@example.test\"\n",
        "password = \"mail-secret\"\n",
        "[smtp]\n",
        "addr = \"smtp.example.net:465\"\n",
        "server_name = \"smtp.example.net\"\n",
    )
}

/// A JMAP-shaped config (no IMAP dial halves).
fn jmap_config_toml() -> &'static str {
    concat!(
        "id = \"cloud\"\n",
        "[jmap]\n",
        "email = \"grace@example.test\"\n",
        "base_url = \"https://jmap.example.test\"\n",
        "password = \"cloud-secret\"\n",
    )
}

fn work() -> AccountId {
    AccountId::try_from("work").expect("an account id")
}

/// The desktop test idiom: poll async halves on the host's own runtime
/// (tokio's block_on is disallowed in this crate).
fn drive<M, F>(make: M) -> F::Output
where
    M: FnOnce() -> F + Send,
    F: std::future::Future,
    F::Output: Send,
{
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(make)
}

fn table(name: &str, tomls: &[&str]) -> Arc<AccountDials> {
    let dials = Arc::new(AccountDials::empty(&scratch(name)));
    for toml in tomls {
        let config = astra_account::load_str(toml).expect("the fixture config loads");
        let id = config.account_id().expect("the fixture pins its id");
        dials
            .register(&id, config, astra_account::ImapConnections::new())
            .expect("the scratch table registers");
    }
    dials
}

fn store(dials: &Arc<AccountDials>, credentials: &Arc<FakeStore>) -> HostAccountStore {
    let credentials =
        Arc::clone(credentials) as Arc<dyn crate::credential_store::AccountCredentialStore>;
    HostAccountStore::new(
        Arc::clone(dials),
        credentials,
        Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
    )
}

#[test]
fn the_method_write_persists_and_the_none_write_drops_the_section() {
    let account = work();
    let dials = table("crypto-write", &[mail_config_toml()]);
    let credentials = FakeStore::accepting();
    credentials
        .persist("work", mail_config_toml())
        .expect("the seed document persists");
    let store = store(&dials, &credentials);

    // No section: the echo is the default.
    let infos = drive(|| store.account_infos());
    assert_eq!(infos[0].crypto_method, "none");

    drive(|| store.store_crypto_method(&account, "openpgp")).expect("the method persists");
    let infos = drive(|| store.account_infos());
    assert_eq!(infos[0].crypto_method, "openpgp");
    let document = newest(&credentials, "work");
    assert!(
        document.contains("[crypto]") && document.contains("method = \"openpgp\""),
        "the persisted document carries the section: {document}"
    );

    // `none` drops the section entirely, so an older document's section
    // does not linger.
    drive(|| store.store_crypto_method(&account, "none")).expect("the none write persists");
    let infos = drive(|| store.account_infos());
    assert_eq!(infos[0].crypto_method, "none");
    assert!(
        !newest(&credentials, "work").contains("[crypto]"),
        "the section is gone"
    );

    // An unregistered account answers the sentence, not a panic.
    let ghost = AccountId::try_from("ghost").expect("an account id");
    let err =
        drive(|| store.store_crypto_method(&ghost, "openpgp")).expect_err("no config to write");
    assert!(err.contains("no stored config"));
}

#[test]
fn the_resolver_answers_none_for_a_config_without_a_section() {
    let account = work();
    let dials = table("crypto-resolve-none", &[mail_config_toml()]);
    let resolver = HostCryptoResolver::new(dials);
    assert_eq!(
        drive(|| resolver.crypto_method(&account)),
        astra_crypto_core::CryptoMethod::None
    );
    assert!(drive(|| resolver.crypto_transport(&account)).is_none());
}

#[test]
fn the_resolver_answers_the_full_dial_table_for_a_crypto_account() {
    let account = work();
    let dials = table("crypto-resolve-full", &[mail_config_toml()]);
    // Write the method into the stored config first (the same half the
    // settings verb drives).
    let credentials = FakeStore::accepting();
    credentials
        .persist("work", mail_config_toml())
        .expect("the seed document persists");
    let store = store(&dials, &credentials);
    drive(|| store.store_crypto_method(&account, "openpgp")).expect("the method persists");

    let resolver = HostCryptoResolver::new(Arc::clone(&dials));
    assert_eq!(
        drive(|| resolver.crypto_method(&account)),
        astra_crypto_core::CryptoMethod::Openpgp
    );
    let transport =
        drive(|| resolver.crypto_transport(&account)).expect("a full dial table resolves");
    assert_eq!(transport.account_email, "ada@example.test");
    assert_eq!(transport.smtp.addr, "smtp.example.net:465");
    assert_eq!(transport.imap.addr, "imap.example.net:993");
    assert_eq!(transport.smtp.username, "ada@example.test");
    assert!(
        matches!(
            transport.smtp.security,
            astra_crypto_store::TransportSecurity::ImplicitTls
        ),
        "the default security is implicit TLS"
    );
}

#[test]
fn a_method_without_dial_halves_is_the_typed_unavailable_answer() {
    let dials = table("crypto-resolve-jmap", &[jmap_config_toml()]);
    // Force a method onto the JMAP config's stored document (the resolver
    // must answer None rather than fabricate a dial table).
    // The fixture pins its id in the document's root, so "cloud" IS the
    // account id (not the derived form).
    let cloud = AccountId::try_from("cloud").expect("the pinned id");
    let credentials = FakeStore::accepting();
    credentials
        .persist("cloud", jmap_config_toml())
        .expect("the seed document persists");
    let store = store(&dials, &credentials);
    drive(|| store.store_crypto_method(&cloud, "smime")).expect("the method persists");

    let resolver = HostCryptoResolver::new(Arc::clone(&dials));
    assert_eq!(
        drive(|| resolver.crypto_method(&cloud)),
        astra_crypto_core::CryptoMethod::Smime
    );
    assert!(
        drive(|| resolver.crypto_transport(&cloud)).is_none(),
        "no IMAP/SMTP submission settings: the typed unavailable answer"
    );
}

/// The newest document the credential store holds for `id`.
fn newest(credentials: &FakeStore, id: &str) -> String {
    let entries = credentials.entries.lock().unwrap();
    entries
        .iter()
        .rev()
        .find(|(held, _)| held == id)
        .map(|(_, toml)| toml.clone())
        .unwrap_or_default()
}
