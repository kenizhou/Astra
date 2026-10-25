// SPDX-License-Identifier: GPL-3.0-only
//! The dial-table token sink's mail arm: a rotation advances the
//! `[imap.oauth]` grant in both the stored document and the dial table's
//! entry, and a mail account's grant is never mistaken for a microsoft,
//! google, or jmap one.

use engine_api::AccountId;

use crate::oauth::graph_sink::graph_token_sink;

use super::*;

/// A registered stored mail grant document, the shape the sign-in's
/// completion persists.
fn register_stored_mail(host: &Host, email: &str) -> String {
    let id = mail_id(email);
    let account = AccountId::try_from(id.as_str()).unwrap();
    host.dials
        .register(
            &account,
            mail_grant_config(),
            astra_account::ImapConnections::new(),
        )
        .expect("the mail account registers");
    id
}

/// A stored mail grant document over the quiet endpoint.
fn mail_grant_config() -> AccountConfig {
    astra_account::load_str(concat!(
        "[imap]\n",
        "addr = \"mail.example.test:993\"\n",
        "server_name = \"mail.example.test\"\n",
        "username = \"alice@example.test\"\n",
        "\n[imap.oauth]\n",
        "client_id = \"client-xyz\"\n",
        "refresh_token = \"rt-1\"\n",
        "authorize_endpoint = \"https://id.example.test/authorize\"\n",
        "token_endpoint = \"https://auth.example.test/token\"\n",
        "redirect_uri = \"com.kenizhou.astra://oauth\"\n",
        "scopes = [\"offline_access\"]\n",
    ))
    .expect("the stored fixture loads")
}

#[test]
fn the_sink_persists_a_rotation_for_a_registered_mail_entry() {
    let host = Host::new("mail-sink-rotation");
    let id = register_stored_mail(&host, "alice@example.test");
    let sink = graph_token_sink(Arc::clone(&host.dials), host.store.clone());
    let account = AccountId::try_from(id.as_str()).unwrap();

    host.runtime.block_on(|| {
        let sink = Arc::clone(&sink);
        let account = account.clone();
        async move {
            sink.refresh_token_rotated(&account, "rotated-9").await;
        }
    });

    let imap = stored_imap(&host, &id);
    assert_eq!(
        imap.oauth.expect("the grant").refresh_token.expose(),
        "rotated-9",
        "the rotation advanced the stored mail grant"
    );
    let entry = host.dials.of(&id).expect("the entry stands").0;
    assert_eq!(
        entry
            .imap
            .expect("the section")
            .oauth
            .expect("the grant")
            .refresh_token
            .expose(),
        "rotated-9",
        "and the dial table's entry with it"
    );
}
