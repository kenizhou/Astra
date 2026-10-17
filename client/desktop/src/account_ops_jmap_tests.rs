// SPDX-License-Identifier: GPL-3.0-only
//! The kind row's jmap arm: a registered `[jmap]` document answers the
//! `jmap` kind and its own address on the settings seam (never the
//! `imap_smtp` default that misnames it), and the removal takes the
//! document away with the row.

use std::sync::Arc;

use astra_viewmodel::AccountKind;

use astra_app::HostAccountOps;

use crate::commands_fakes::{scratch, FakeStore};
use crate::credential_store::AccountCredentialStore;
use crate::dial_table::AccountDials;

use super::HostAccountStore;

/// A stored `[jmap]` password document that pins its id.
fn jmap_config_toml() -> &'static str {
    concat!(
        "id = \"jmap-work\"\n",
        "[jmap]\n",
        "email = \"ada@example.test\"\n",
        "base_url = \"https://mail.example.test\"\n",
        "password = \"mail-secret\"\n",
    )
}

/// A stored `[jmap]` grant document that pins its id, the shape the
/// sign-in's completion persists.
fn jmap_grant_config_toml() -> &'static str {
    concat!(
        "id = \"jmap-signed-in\"\n",
        "[jmap]\n",
        "email = \"grace@example.test\"\n",
        "base_url = \"https://mail.example.test\"\n",
        "\n[jmap.oauth]\n",
        "client_id = \"client-abc\"\n",
        "refresh_token = \"rt-1\"\n",
        "authorize_endpoint = \"https://mail.example.test/oauth/authorize\"\n",
        "token_endpoint = \"https://auth.example.test/token\"\n",
        "redirect_uri = \"com.kenizhou.astra://oauth\"\n",
        "scopes = [\"offline_access\"]\n",
    )
}

/// A scratch dial table holding the two jmap fixtures.
fn table(name: &str) -> Arc<AccountDials> {
    let dials = Arc::new(AccountDials::empty(&scratch(name)));
    for toml in [jmap_config_toml(), jmap_grant_config_toml()] {
        let config = astra_account::load_str(toml).expect("the fixture config loads");
        let id = config.account_id().expect("the fixture pins its id");
        dials
            .register(&id, config, astra_account::ImapConnections::new())
            .expect("the scratch table registers");
    }
    dials
}

/// The store under test over the jmap fixtures.
fn store(dials: &Arc<AccountDials>) -> (HostAccountStore, Arc<FakeStore>) {
    let credentials = FakeStore::accepting();
    for (id, toml) in [
        ("jmap-work", jmap_config_toml()),
        ("jmap-signed-in", jmap_grant_config_toml()),
    ] {
        credentials
            .persist(id, toml)
            .expect("the seed document persists");
    }
    let ops = HostAccountStore::new(
        Arc::clone(dials),
        Arc::clone(&credentials) as Arc<dyn AccountCredentialStore>,
        Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
    );
    (ops, credentials)
}

#[test]
fn a_registered_jmap_document_answers_the_jmap_kind_and_its_address() {
    let dials = table("ops-jmap-kind");
    let (ops, _credentials) = store(&dials);

    let infos = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| ops.account_infos());

    let mut by_address: Vec<(AccountKind, String)> = infos
        .iter()
        .map(|info| (info.kind, info.address.clone()))
        .collect();
    by_address.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(
        by_address,
        vec![
            (AccountKind::Jmap, "ada@example.test".to_owned()),
            (AccountKind::Jmap, "grace@example.test".to_owned()),
        ],
        "both jmap shapes answer the jmap kind and their own addresses"
    );
}

#[test]
fn the_removal_takes_a_jmap_account_s_document_and_row_away() {
    let dials = table("ops-jmap-remove");
    let (ops, credentials) = store(&dials);
    let id = engine_api::AccountId::try_from("jmap-work").expect("an account id");

    let removed = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| ops.remove_account(&id));
    removed.expect("the removal lands");

    assert!(
        credentials.stored("jmap-work").is_none(),
        "the document is gone from the secure store"
    );
    assert!(
        dials.of("jmap-work").is_none(),
        "the dial table's entry is gone with it"
    );
    assert!(
        crate::runtime::build_runtime()
            .expect("the test runtime builds")
            .block_on(|| ops.account_infos())
            .iter()
            .all(|info| info.id != id),
        "the row renders no more"
    );
}
