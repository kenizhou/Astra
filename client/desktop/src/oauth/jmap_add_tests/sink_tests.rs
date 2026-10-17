// SPDX-License-Identifier: GPL-3.0-only
//! The dial-table token sink's jmap arm: a rotation advances the
//! `[jmap.oauth]` grant in both the stored document and the dial table's
//! entry, and a jmap account's grant is never mistaken for a
//! microsoft or google one. A moved scope set writes nothing for a jmap
//! account: its grant carries no granted-set field to move.

use astra_oauth::GrantedScopes;

use crate::oauth::graph_sink::graph_token_sink;

use super::*;

/// A registered stored jmap grant document, the shape the sign-in's
/// completion persists.
fn register_stored_jmap(host: &Host, email: &str) -> String {
    let id = jmap_id(email);
    let account = AccountId::try_from(id.as_str()).unwrap();
    host.dials
        .register(
            &account,
            astra_account::load_str(&grant_document(email, QUIET_TOKEN_ENDPOINT))
                .expect("the stored fixture loads"),
            astra_account::ImapConnections::new(),
        )
        .expect("the jmap account registers");
    id
}

#[test]
fn the_sink_persists_a_rotation_for_a_registered_jmap_entry() {
    let host = Host::new("jmap-sink-rotation");
    let id = register_stored_jmap(&host, "alice@example.test");
    let sink = graph_token_sink(Arc::clone(&host.dials), host.store.clone());
    let account = AccountId::try_from(id.as_str()).unwrap();

    host.runtime.block_on(|| {
        let sink = Arc::clone(&sink);
        let account = account.clone();
        async move {
            sink.refresh_token_rotated(&account, "rotated-9").await;
        }
    });

    let jmap = stored_jmap(&host, &id);
    assert_eq!(
        jmap.oauth.expect("the grant").refresh_token.expose(),
        "rotated-9",
        "the rotation advanced the stored jmap grant"
    );
    let entry = host.dials.of(&id).expect("the entry stands").0;
    assert_eq!(
        entry
            .jmap
            .expect("the section")
            .oauth
            .expect("the grant")
            .refresh_token
            .expose(),
        "rotated-9",
        "and the dial table's entry with it"
    );
}

#[test]
fn the_sink_leaves_a_jmap_entry_s_grant_alone_on_a_scope_note() {
    let host = Host::new("jmap-sink-scopes");
    let id = register_stored_jmap(&host, "alice@example.test");
    let sink = graph_token_sink(Arc::clone(&host.dials), host.store.clone());
    let account = AccountId::try_from(id.as_str()).unwrap();
    let granted = GrantedScopes::from_stored(vec!["offline_access".to_owned()]);

    host.runtime.block_on(|| {
        let sink = Arc::clone(&sink);
        let account = account.clone();
        let granted = granted.clone();
        async move {
            sink.scopes_granted(&account, &granted).await;
        }
    });

    assert!(
        host.store.entries.lock().unwrap().is_empty(),
        "a jmap grant has no granted set to move, so nothing is written"
    );
}
