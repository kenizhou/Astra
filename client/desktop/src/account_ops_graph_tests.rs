// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the host's account store over a stored graph grant: the
//! info rows a Microsoft account answers (the Microsoft kind, the
//! address the sign-in's lookup named, the standing refusal state the
//! refusal surfaces render) and the removal's refusal bookkeeping.

use std::sync::Arc;

use astra_app::HostAccountOps;
use engine_api::AccountId;

use super::HostAccountStore;
use crate::commands_fakes::{scratch, FakeStore};
use crate::dial_table::AccountDials;

/// A stored graph-only config, the shape a completed Microsoft sign-in
/// leaves behind.
fn graph_config_toml() -> &'static str {
    concat!(
        "[graph]\n",
        "email = \"alice@example.test\"\n",
        "client_id = \"client-abc\"\n",
        "tenant = \"common\"\n",
        "redirect_uri = \"com.kenizhou.astra://oauth\"\n",
        "scopes = [\"offline_access\"]\n",
        "refresh_token = \"old-refresh\"\n",
    )
}

/// A scratch dial table holding one registered graph account, and the
/// account's id.
fn graph_table(name: &str) -> (Arc<AccountDials>, AccountId) {
    let dials = Arc::new(AccountDials::empty(&scratch(name)));
    let account = AccountId::try_from("alice@example.test@graph").expect("an account id");
    dials
        .register(
            &account,
            astra_account::load_str(graph_config_toml()).expect("the graph config loads"),
            astra_account::ImapConnections::new(),
        )
        .expect("the graph account registers");
    (dials, account)
}

#[test]
fn account_infos_answer_the_microsoft_kind_the_graph_address_and_the_standing_refusal() {
    let (dials, _account) = graph_table("infos-graph");
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());
    refusals.record(
        "alice@example.test@graph",
        astra_viewmodel::GrantState::CalendarWithheld,
    );
    let ops = HostAccountStore::new(
        Arc::clone(&dials),
        FakeStore::accepting(),
        Arc::clone(&refusals),
        crate::commands_fakes::crypto_store(),
    );

    let infos = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| ops.account_infos());

    let info = infos
        .iter()
        .find(|info| info.id.as_str() == "alice@example.test@graph")
        .expect("the graph account's info stands");
    assert_eq!(
        info.kind,
        astra_viewmodel::AccountKind::Microsoft,
        "a stored grant is a Microsoft account, never an imap row"
    );
    assert_eq!(
        info.address, "alice@example.test",
        "the info names the address the grant's lookup answered"
    );
    assert_eq!(
        info.grant,
        Some(astra_viewmodel::GrantState::CalendarWithheld),
        "the standing refusal state rides the info the rows render"
    );
}

#[test]
fn a_removed_account_s_refusal_goes_with_the_row() {
    let (dials, account) = graph_table("infos-remove-graph");
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());
    refusals.record(
        "alice@example.test@graph",
        astra_viewmodel::GrantState::SignInExpired,
    );
    let ops = HostAccountStore::new(
        Arc::clone(&dials),
        FakeStore::accepting(),
        Arc::clone(&refusals),
        crate::commands_fakes::crypto_store(),
    );

    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| ops.remove_account(&account))
        .expect("the removal lands");

    assert_eq!(
        refusals.of("alice@example.test@graph"),
        None,
        "a removed account owes no refusal state"
    );
}
