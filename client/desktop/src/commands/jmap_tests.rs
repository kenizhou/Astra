// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the wizard's JMAP add command: the one derivation the host
//! owns (a blank server URL derives the email's domain, a bare host
//! gains `https://`, an explicit scheme survives) runs host-side in
//! `astra-account`, the dial runs before anything is stored, and the
//! store's refusal rolls the whole add back. The dial is a stand-in that
//! captures the config it was handed; no test touches a JMAP server.

use std::sync::Arc;

use super::add_jmap_account_with;
use crate::app_state::HostProvider;
use crate::commands::{AddedAccount, CommandError};
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// A dial that succeeds, recording the config it was handed.
fn ok_dial(
    seen: &Arc<std::sync::Mutex<Vec<astra_account::AccountConfig>>>,
) -> impl FnOnce(
    &astra_account::AccountConfig,
    &engine_api::AccountId,
) -> Result<
    (Vec<HostProvider>, Arc<astra_account::ImapConnections>),
    astra_account::AccountError,
> {
    let seen = Arc::clone(seen);
    move |config, _id| {
        seen.lock().unwrap().push(config.clone());
        Ok((Vec::new(), astra_account::ImapConnections::new()))
    }
}

#[test]
fn a_jmap_setup_dials_persists_and_registers_the_account() {
    let app = FakeApp::new();
    let store = FakeStore::accepting();
    let st = state("jmap-add-ok", Arc::clone(&app), Arc::clone(&store));
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));

    let added = add_jmap_account_with(
        &st,
        "Added@Example.test",
        Some("https://mail.example.test"),
        "an-api-token",
        ok_dial(&seen),
    )
    .expect("the account is added");

    assert_eq!(
        added,
        AddedAccount {
            account_id: "added@example.test@jmap:mail.example.test".to_owned(),
            email: "Added@Example.test".to_owned(),
        },
        "the id keys the address under the jmap scheme and the server host"
    );
    let dialled = seen.lock().unwrap().pop().expect("the dial ran");
    let jmap = dialled
        .jmap
        .expect("the dialled config carries the section");
    assert_eq!(jmap.base_url, "https://mail.example.test");
    assert_eq!(
        jmap.password.expect("the one secret").expose(),
        "an-api-token"
    );

    let stored = store
        .stored("added@example.test@jmap:mail.example.test")
        .expect("the document is stored under the account id");
    let parsed = astra_account::load_str(&stored).expect("the stored document loads");
    assert_eq!(
        parsed.jmap.expect("the section").email,
        "Added@Example.test",
        "the stored document carries the [jmap] section, password and all"
    );
    assert_eq!(
        st.connections.ids(),
        vec!["added@example.test@jmap:mail.example.test".to_owned()],
        "the dial table registers the account"
    );
    assert!(
        app.added
            .lock()
            .unwrap()
            .contains(&"added@example.test@jmap:mail.example.test".to_owned()),
        "the app registers the account"
    );
}

#[test]
fn a_blank_server_url_derives_the_email_domain_host_side() {
    let app = FakeApp::new();
    let store = FakeStore::accepting();
    let st = state("jmap-add-derived", Arc::clone(&app), Arc::clone(&store));

    let added = add_jmap_account_with(
        &st,
        "added@example.test",
        None,
        "a-password",
        ok_dial(&Arc::new(std::sync::Mutex::new(Vec::new()))),
    )
    .expect("the account is added");

    let stored = store
        .stored(added.account_id.as_str())
        .expect("the document is stored");
    let parsed = astra_account::load_str(&stored).expect("the stored document loads");
    assert_eq!(
        parsed.jmap.expect("the section").base_url,
        "https://example.test",
        "a blank server derives the email domain over https"
    );
}

#[test]
fn a_bare_host_gains_https_and_a_public_http_is_upgraded() {
    let store = FakeStore::accepting();
    let st = state("jmap-add-normalised", FakeApp::new(), Arc::clone(&store));

    for (input, stored_url) in [
        ("mail.example.test", "https://mail.example.test"),
        ("http://mail.example.test", "https://mail.example.test"),
        ("http://127.0.0.1:2080", "http://127.0.0.1:2080"),
    ] {
        let added = add_jmap_account_with(
            &st,
            "added@example.test",
            Some(input),
            "a-password",
            ok_dial(&Arc::new(std::sync::Mutex::new(Vec::new()))),
        )
        .expect("the account is added");
        let stored = store
            .stored(added.account_id.as_str())
            .expect("the document is stored");
        let parsed = astra_account::load_str(&stored).expect("the stored document loads");
        assert_eq!(
            parsed.jmap.expect("the section").base_url,
            stored_url,
            "the derivation matrix runs host-side for {input}"
        );
    }
}

#[test]
fn a_refused_dial_stores_nothing() {
    let app = FakeApp::new();
    let store = FakeStore::accepting();
    let st = state(
        "jmap-add-refused-dial",
        Arc::clone(&app),
        Arc::clone(&store),
    );

    let error = add_jmap_account_with(
        &st,
        "added@example.test",
        Some("https://mail.example.test"),
        "a-password",
        |_config, _id| {
            Err(astra_account::AccountError::SigninRejected(
                "the server refused the password".to_owned(),
            ))
        },
    )
    .expect_err("the dial's refusal is the add's");

    assert!(
        matches!(error, CommandError::Connect(ref reason) if reason.contains("sign-in")),
        "the error is typed and carries the reason: {error:?}"
    );
    assert!(
        store.entries.lock().unwrap().is_empty(),
        "nothing was stored over a refused dial"
    );
    assert!(
        st.connections.ids().is_empty(),
        "nothing was registered over a refused dial"
    );
}

#[test]
fn an_empty_password_is_a_config_error() {
    let app = FakeApp::new();
    let store = FakeStore::accepting();
    let st = state("jmap-add-empty", Arc::clone(&app), Arc::clone(&store));

    let error = add_jmap_account_with(
        &st,
        "added@example.test",
        Some("https://mail.example.test"),
        "   ",
        |_config, _id| unreachable!("a blank secret never dials"),
    )
    .expect_err("the blank secret refuses the add");

    assert!(
        matches!(error, CommandError::Config(ref said) if said.contains("password")),
        "the error names the missing field: {error:?}"
    );
    assert!(store.entries.lock().unwrap().is_empty());
}
