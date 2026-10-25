// SPDX-License-Identifier: GPL-3.0-only
//! The re-consent path's tests: a sign-in for an account that stands
//! keeps its stored shape and renews through the rotation path, and a
//! failed dial restores the standing entry and pool exactly as they
//! stood, so a live account keeps dialling.

use std::sync::Arc;

use engine_api::AccountId;

use astra_account::{AccountConfig, ImapConnections};

use super::*;

/// A standing mail account's stored shape: a pinned id, a dav section,
/// and an accepted certificate the grant does not own.
fn standing_config() -> AccountConfig {
    astra_account::load_str(concat!(
        "id = \"alice@example.test@mail.example.test\"\n",
        "[imap]\n",
        "addr = \"mail.example.test:993\"\n",
        "server_name = \"mail.example.test\"\n",
        "username = \"Alice@Example.test\"\n",
        "\n[imap.oauth]\n",
        "client_id = \"client-old\"\n",
        "refresh_token = \"rt-old\"\n",
        "authorize_endpoint = \"https://id.example.test/authorize\"\n",
        "token_endpoint = \"https://auth.example.test/token\"\n",
        "redirect_uri = \"com.kenizhou.astra://oauth\"\n",
        "scopes = [\"offline_access\"]\n",
        "\n[dav]\n",
        "caldav_url = \"https://dav.example.test\"\n",
        "username = \"Alice@Example.test\"\n",
        "password = \"dav-secret\"\n",
        "\n[[certificate_exception]]\n",
        "server_name = \"mail.example.test\"\n",
        "sha256 = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n",
    ))
    .expect("the standing fixture loads")
}

/// Registers a standing account under `email`'s derived id, answering
/// the pool it registered with so a rollback's restoration is
/// observable.
fn register_stored(host: &Host, config: AccountConfig, email: &str) -> Arc<ImapConnections> {
    let id = AccountId::try_from(mail_id(email).as_str()).unwrap();
    let pool = ImapConnections::new();
    host.dials
        .register(&id, config, Arc::clone(&pool))
        .expect("the standing account registers");
    pool
}

#[test]
fn a_sign_in_for_an_account_that_stands_keeps_its_shape_and_rotates() {
    let host = Host::new("mail-add-reconsent");
    register_stored(&host, standing_config(), "Alice@Example.test");
    let id = mail_id("Alice@Example.test");

    let address = complete(
        &host,
        exchanged_document(),
        ok_dial(Arc::new(DialSaw::default())),
    )
    .expect("the re-consent completes");

    assert_eq!(address, "alice@example.test");
    assert_eq!(
        stored_documents(&host, &id).len(),
        1,
        "the re-consent persists once, through the rotation path"
    );

    let stored = host.store.stored(&id).expect("the renewed grant persists");
    let config = astra_account::load_str(&stored).expect("the document loads");
    let imap = config.imap.expect("the imap section stands");
    assert_eq!(
        imap.oauth.expect("the grant").refresh_token.expose(),
        "rt-9",
        "the renewed grant replaced the old one"
    );
    assert!(imap.password.is_none(), "the re-consent writes no password");
    assert_eq!(
        config.shape.id.map(|pinned| pinned.as_str().to_owned()),
        Some(id.clone()),
        "the stored shape keeps its pinned id"
    );
    assert!(
        config.dav.is_some(),
        "the dav section the grant does not own stays"
    );
    assert_eq!(
        config.certificate_exceptions.len(),
        1,
        "the accepted certificate the grant does not own stays"
    );
    let joined = wait_for(|| host.app.added.lock().unwrap().first().cloned());
    assert_eq!(joined, id, "the renewed account re-joins the app in place");
}

#[test]
fn a_failed_dial_on_a_reconsent_restores_the_standing_entry_and_pool() {
    let host = Host::new("mail-add-reconsent-dial-failed");
    let standing_pool = register_stored(&host, standing_config(), "Alice@Example.test");
    let id = mail_id("Alice@Example.test");

    complete(
        &host,
        exchanged_document(),
        failing_dial(Arc::new(DialSaw::default()), || {
            AccountError::MailOauth("the connect never answered".to_owned())
        }),
    )
    .expect_err("the failed dial refuses the re-consent");

    let (entry, pool) = host.dials.of(&id).expect("the standing entry stays");
    let imap = entry.imap.expect("the imap section");
    assert_eq!(
        imap.oauth.expect("the grant").refresh_token.expose(),
        "rt-old",
        "the entry is put back as it stood, not left on the new grant"
    );
    assert!(
        Arc::ptr_eq(&pool, &standing_pool),
        "the standing entry's pool is put back with it, so the live account keeps dialling"
    );
}
