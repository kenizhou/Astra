// SPDX-License-Identifier: GPL-3.0-only
//! The dial-table token sink on its own: a rotation advances both the
//! stored grant and the dial table's entry, a moved scope set is recorded
//! over the stored one, and a set that did not move writes nothing.

use astra_oauth::GrantedScopes;

use crate::oauth::graph_sink::graph_token_sink;

use super::*;

fn stored_config_for(email: &str) -> AccountConfig {
    astra_account::load_str(&format!(
        concat!(
            "[graph]\n",
            "email = \"{email}\"\n",
            "client_id = \"client-abc\"\n",
            "tenant = \"common\"\n",
            "redirect_uri = \"com.kenizhou.astra://oauth\"\n",
            "scopes = [\"offline_access\"]\n",
            "refresh_token = \"old-refresh\"\n",
            "granted_scopes = [\"offline_access\"]\n",
        ),
        email = email,
    ))
    .expect("the stored fixture loads")
}

#[test]
fn the_sink_persists_a_rotation_for_the_registered_entry() {
    let host = Host::new("graph-sink-rotation");
    let id = graph_id("alice@example.test");
    register_stored(
        &host,
        stored_config_for("alice@example.test"),
        "alice@example.test",
    );
    let sink = graph_token_sink(Arc::clone(&host.dials), host.store.clone());
    let account = AccountId::try_from(id.as_str()).unwrap();

    host.runtime.block_on(|| {
        let sink = Arc::clone(&sink);
        let account = account.clone();
        async move {
            sink.refresh_token_rotated(&account, "rotated-9").await;
        }
    });

    let graph = stored_graph(&host, &id);
    assert_eq!(
        graph.refresh_token.expose(),
        "rotated-9",
        "the rotation advanced the stored grant"
    );
    let entry = host.dials.of(&id).expect("the entry stands").0;
    assert_eq!(
        entry.graph.expect("the grant").refresh_token.expose(),
        "rotated-9",
        "and the dial table's entry with it"
    );
}

#[test]
fn the_sink_records_a_moved_scope_set() {
    let host = Host::new("graph-sink-scopes");
    let id = graph_id("alice@example.test");
    register_stored(
        &host,
        stored_config_for("alice@example.test"),
        "alice@example.test",
    );
    let sink = graph_token_sink(Arc::clone(&host.dials), host.store.clone());
    let account = AccountId::try_from(id.as_str()).unwrap();
    let granted = GrantedScopes::from_stored(vec![
        "offline_access".to_owned(),
        "https://graph.microsoft.com/Mail.ReadWrite".to_owned(),
    ]);

    host.runtime.block_on(|| {
        let sink = Arc::clone(&sink);
        let account = account.clone();
        let granted = granted.clone();
        async move {
            sink.scopes_granted(&account, &granted).await;
        }
    });

    let graph = stored_graph(&host, &id);
    assert_eq!(
        graph.granted_scopes,
        Some(vec![
            "offline_access".to_owned(),
            "https://graph.microsoft.com/Mail.ReadWrite".to_owned(),
        ]),
        "a moved scope set is recorded over the stored one"
    );
}

#[test]
fn the_sink_leaves_an_unchanged_scope_set_alone() {
    let host = Host::new("graph-sink-scopes-same");
    let id = graph_id("alice@example.test");
    register_stored(
        &host,
        stored_config_for("alice@example.test"),
        "alice@example.test",
    );
    let sink = graph_token_sink(Arc::clone(&host.dials), host.store.clone());
    let account = AccountId::try_from(id.as_str()).unwrap();
    // The stored set for the fixture is the offline_access-only default.
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
        "a set that did not move writes nothing"
    );
}
