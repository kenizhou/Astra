// SPDX-License-Identifier: GPL-3.0-only
//! The gated sender-identity proof: the outgoing-name verbs over the
//! harness's real JMAP, the one family whose identity controls are
//! writable (Google's are writable but needs a real registration;
//! Microsoft's are read-only; IMAP owns none). The read half pins what
//! the server's own `Identity/get` answers (the harness's identity
//! carries the account's own address, its name as Stalwart holds it),
//! the write half pins the round trip: `set_sender_name` lands on the
//! real server and the next read carries the new name. The adopt-at-join
//! leg rides the same read the app's `adopt_sender_name` drives, so the
//! proof pins the whole family with no scripted server anywhere.

use std::sync::Arc;

use engine_api::{Engine, Provider};

use crate::{Account, App, AppObserver};

use super::{harness, jmap_base, CountingObserver};

/// The sender-identity round trip over the harness's JMAP: the read
/// answers the account's own identity (Stalwart serves one identity per
/// account, its address the account's own), the write lands the new name
/// on the real server, and the next read carries it back. The adopt-at-
/// join leg rides the same read, so the proof covers the whole writable
/// family: adopt where a name answers, and the write where the person
/// sets one.
#[tokio::test]
async fn the_sender_name_round_trips_over_the_harness_jmap() {
    let Some((_host, _port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let base = jmap_base().expect("the gated run names the harness origin");

    let config = astra_account::AccountConfig {
        imap: None,
        smtp: None,
        certificate_exceptions: Vec::new(),
        graph: None,
        google: None,
        jmap: Some(astra_account::JmapAccount {
            email: user.clone(),
            base_url: base,
            password: Some(astra_account::Secret::new(password.clone())),
            oauth: None,
        }),
        dav: None,
        crypto: None,
        shape: astra_account::AccountShape::default(),
    };
    let account_id = config.account_id().expect("the jmap account id derives");
    let tls = engine_tls::client_config(&engine_tls::TlsPolicy::bundled_and_system())
        .expect("the account tls policy builds");
    let provider = astra_account::dial_jmap_account(&config, &tls)
        .await
        .expect("the jmap dial over the harness origin");

    // The read half: the server answers the account's own identity.
    let identities = provider
        .sender_identities(&account_id)
        .await
        .expect("the identity read over the real server");
    let identity = identities
        .iter()
        .find(|identity| identity.address.email.eq_ignore_ascii_case(user.as_str()))
        .expect("the server answers the account's own identity");

    // The write half: the name lands on the real server and reads back.
    provider
        .set_sender_name(&account_id, &identity.id, "Ada Lovelace")
        .await
        .expect("the name write over the real server");
    let after = provider
        .sender_identities(&account_id)
        .await
        .expect("the identity re-read after the write");
    let written = after
        .iter()
        .find(|row| row.id == identity.id)
        .expect("the written identity stands after the re-read");
    assert_eq!(
        written.address.name.as_deref(),
        Some("Ada Lovelace"),
        "the written name reads back from the real server"
    );

    // The adopt leg rides the same read: the app-level adoption over the
    // dialled provider answers the written name (the guard keeps nothing
    // stored in this fresh app, so the adoption runs and finds it).
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account_id.clone(),
            providers: vec![Box::new(
                astra_account::dial_jmap_account(&config, &tls)
                    .await
                    .expect("the fresh dial for the app"),
            )],
            identity: engine_api::EmailAddress::new(user.as_str()),
        }],
        None,
        observer,
    );
    app.sync_added_account(&account_id).await;
    let row = &app.settings().accounts[0];
    assert_eq!(
        row.display_name, "Ada Lovelace",
        "the join's adoption reads the written name the server holds"
    );
    assert!(
        !row.needs_sender_name,
        "the adoption spends the ask where the server answers a name"
    );
    assert_eq!(
        format!("{:?}", row.sender_name_control).to_lowercase(),
        "writable",
        "the JMAP family's control is writable"
    );

    // The teardown: the written name is the account's own identity
    // metadata on the harness (no mail, no folders touched), and the
    // harness re-seeds its identity state on the next `harness.sh reset`,
    // so the proof withdraws nothing; the written name is per-account
    // server state the harness treats as scratch, recorded here.
}
