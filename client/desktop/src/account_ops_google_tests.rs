// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the host's account store over a stored google grant: the
//! info rows a Google account answers (the Google kind, the address the
//! sign-in's lookup named, the standing refusal state the expired row
//! renders), never the imap row a stored grant cannot be.

use std::sync::Arc;

use astra_app::HostAccountOps;

use super::HostAccountStore;
use crate::commands_fakes::{scratch, FakeStore};
use crate::dial_table::AccountDials;

/// A stored google-only config, the shape a completed Google sign-in
/// leaves behind (the Desktop secret beside the grant).
fn google_config_toml() -> &'static str {
    concat!(
        "[google]\n",
        "email = \"alice@gmail.example\"\n",
        "client_id = \"google-client\"\n",
        "client_secret = \"gsecret\"\n",
        "redirect_uri = \"http://127.0.0.1:45001/\"\n",
        "scopes = [\"https://mail.google.com/\"]\n",
        "refresh_token = \"old-refresh\"\n",
    )
}

/// A scratch dial table holding one registered google account, and the
/// account's id.
fn google_table(name: &str) -> (Arc<AccountDials>, String) {
    let dials = Arc::new(AccountDials::empty(&scratch(name)));
    let account =
        engine_api::AccountId::try_from("alice@gmail.example@google").expect("an account id");
    dials
        .register(
            &account,
            astra_account::load_str(google_config_toml()).expect("the google config loads"),
            astra_account::ImapConnections::new(),
        )
        .expect("the google account registers");
    (dials, "alice@gmail.example@google".to_owned())
}

#[test]
fn account_infos_answer_the_google_kind_the_grant_s_address_and_the_standing_refusal() {
    let (dials, id) = google_table("infos-google");
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());
    refusals.record(&id, astra_viewmodel::GrantState::SignInExpired);
    let ops = HostAccountStore::new(dials, FakeStore::accepting(), Arc::clone(&refusals));

    let infos = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| ops.account_infos());

    let info = infos
        .iter()
        .find(|info| info.id.as_str() == "alice@gmail.example@google")
        .expect("the google account's info stands");
    assert_eq!(
        info.kind,
        astra_viewmodel::AccountKind::Google,
        "a stored google grant is a Google account, never an imap row"
    );
    assert_eq!(
        info.address, "alice@gmail.example",
        "the info names the address the grant's lookup answered"
    );
    assert_eq!(
        info.grant,
        Some(astra_viewmodel::GrantState::SignInExpired),
        "the standing refusal state rides the info the expired row renders"
    );
}
