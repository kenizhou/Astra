// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the crypto halves of the host's wrapper factory and account
//! store (ECS): the factory answers a wrapper exactly when a method and an
//! IMAP dial stand, the method write re-dials BEFORE it persists (the new
//! wrapper is what the next send drains against), and a method on an
//! account without both dial halves is refused fail-closed.

use std::sync::Arc;

use engine_api::AccountId;

use astra_app::HostAccountOps;

use super::mime_wrapper_for_config;
use crate::account_ops::HostAccountStore;
use crate::commands_fakes::{crypto_store, scratch, FakeStore};
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

fn ops_store(dials: &Arc<AccountDials>, credentials: &Arc<FakeStore>) -> HostAccountStore {
    let credentials =
        Arc::clone(credentials) as Arc<dyn crate::credential_store::AccountCredentialStore>;
    HostAccountStore::new(
        Arc::clone(dials),
        credentials,
        Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
        crypto_store(),
    )
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

// --- The wrapper factory ---

#[test]
fn a_config_without_a_section_installs_no_wrapper() {
    let config = astra_account::load_str(mail_config_toml()).expect("the fixture loads");
    assert!(mime_wrapper_for_config(&crypto_store(), &config, &work()).is_none());
}

#[test]
fn a_method_and_an_imap_dial_install_a_wrapper() {
    let mut config = astra_account::load_str(mail_config_toml()).expect("the fixture loads");
    config.crypto = Some(astra_account::CryptoAccount {
        method: "openpgp".to_owned(),
    });
    assert!(mime_wrapper_for_config(&crypto_store(), &config, &work()).is_some());
}

#[test]
fn a_method_without_an_imap_dial_installs_no_wrapper() {
    let mut config = astra_account::load_str(jmap_config_toml()).expect("the fixture loads");
    config.crypto = Some(astra_account::CryptoAccount {
        method: "openpgp".to_owned(),
    });
    let cloud = AccountId::try_from("cloud").expect("the pinned id");
    assert!(
        mime_wrapper_for_config(&crypto_store(), &config, &cloud).is_none(),
        "no seam to hang it on; the settings write is the guard that keeps this config from standing"
    );
}

// --- The method write ---

#[test]
fn the_write_redials_with_the_new_wrapper_before_it_persists() {
    let account = work();
    let dials = table("crypto-write", &[mail_config_toml()]);
    let credentials = FakeStore::accepting();
    credentials
        .persist("work", mail_config_toml())
        .expect("the seed document persists");
    let store = ops_store(&dials, &credentials);

    // No section: the echo is the default.
    let infos = drive(|| store.account_infos());
    assert_eq!(infos[0].crypto_method, "none");

    // The injected dial records what it was handed and what the document
    // store held AT THAT MOMENT: the re-dial must run before the persist.
    let seen = Arc::new(std::sync::Mutex::new((false, String::new())));
    let seen_in_dial = Arc::clone(&seen);
    let credentials_in_dial = Arc::clone(&credentials);
    let providers = drive(|| {
        store.store_crypto_method_with(&account, "openpgp", move |config, _connections, wrapper| {
            let document_at_dial = credentials_in_dial.stored("work").unwrap_or_default();
            *seen_in_dial.lock().unwrap() = (wrapper.is_some(), document_at_dial);
            assert!(
                config.crypto.is_some(),
                "the dial sees the merged config, method and all"
            );
            async move { Ok(Vec::new()) }
        })
    })
    .expect("the method persists");
    assert!(providers.is_some(), "an IMAP account answers its re-dial");

    let (wrapper_seen, document_at_dial) = &*seen.lock().unwrap();
    assert!(wrapper_seen, "the dial was handed the new method's wrapper");
    assert!(
        !document_at_dial.contains("[crypto]"),
        "the dial ran BEFORE the persist: {document_at_dial}"
    );
    let document = newest(&credentials, "work");
    assert!(
        document.contains("[crypto]") && document.contains("method = \"openpgp\""),
        "the persisted document carries the section: {document}"
    );

    // Clearing re-dials too, with NO wrapper, and drops the section.
    let cleared = drive(|| {
        store.store_crypto_method_with(&account, "none", move |config, _connections, wrapper| {
            assert!(config.crypto.is_none());
            assert!(wrapper.is_none(), "a cleared method installs nothing");
            async move { Ok(Vec::new()) }
        })
    })
    .expect("the none write persists");
    assert!(cleared.is_some());
    assert!(
        !newest(&credentials, "work").contains("[crypto]"),
        "the section is gone"
    );

    // An unregistered account answers the sentence, not a panic.
    let ghost = AccountId::try_from("ghost").expect("an account id");
    let refused = drive(|| {
        store.store_crypto_method_with(
            &ghost,
            "openpgp",
            move |_config, _connections, _wrapper| async move { Ok(Vec::new()) },
        )
    });
    let Err(err) = refused else {
        panic!("no config to write")
    };
    assert!(err.contains("no stored config"));
}

#[test]
fn a_method_on_an_account_without_both_dial_halves_is_refused_fail_closed() {
    let dials = table("crypto-jmap-refused", &[jmap_config_toml()]);
    let cloud = AccountId::try_from("cloud").expect("the pinned id");
    let credentials = FakeStore::accepting();
    credentials
        .persist("cloud", jmap_config_toml())
        .expect("the seed document persists");
    let store = ops_store(&dials, &credentials);

    let refused = drive(|| {
        store.store_crypto_method_with(&cloud, "smime", move |_config, _connections, _wrapper| async move {
            panic!("a refused write never dials")
        })
    });
    let Err(err) = refused else {
        panic!("the write is refused")
    };
    assert_eq!(err, "encryption needs an IMAP/SMTP account");
    assert!(
        !newest(&credentials, "cloud").contains("[crypto]"),
        "nothing changed stored"
    );

    // Clearing a legacy section on the same account needs no seam and no
    // re-dial: it persists and answers none.
    let with_legacy = concat!(
        "id = \"cloud\"\n",
        "[jmap]\n",
        "email = \"grace@example.test\"\n",
        "base_url = \"https://jmap.example.test\"\n",
        "password = \"cloud-secret\"\n",
        "[crypto]\n",
        "method = \"smime\"\n",
    );
    let dials = table("crypto-jmap-clear", &[with_legacy]);
    let credentials = FakeStore::accepting();
    credentials
        .persist("cloud", with_legacy)
        .expect("the legacy document persists");
    let store = ops_store(&dials, &credentials);
    let cleared = drive(|| {
        store.store_crypto_method_with(
            &cloud,
            "none",
            move |_config, _connections, _wrapper| async move {
                panic!("a clear on a grant account re-dials nothing")
            },
        )
    })
    .expect("the clear persists");
    assert!(cleared.is_none(), "no IMAP dial: no re-dialled providers");
    assert!(
        !newest(&credentials, "cloud").contains("[crypto]"),
        "the legacy section is gone"
    );
}
