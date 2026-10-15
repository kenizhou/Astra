// SPDX-License-Identifier: GPL-3.0-only
//! The removal half of the host account store's tests: the config
//! document and the credential go together, and a refusal keeps every
//! entry standing. A child of the seam's own tests module, sharing its
//! table and fixture helpers through `super`.

use std::sync::Arc;

use astra_app::HostAccountOps;
use engine_api::AccountId;

use super::{dav_config_toml, ops_over, table, work, HostAccountStore};
use crate::commands_fakes::FakeStore;
use crate::credential_store::AccountCredentialStore;

#[test]
fn remove_drops_the_config_document_and_the_credential() {
    let dials = table("remove");
    let credentials = FakeStore::accepting();
    credentials
        .persist("work", dav_config_toml())
        .expect("the seed document persists");
    let ops = ops_over(&dials, &credentials);
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let account = work();

    runtime
        .block_on(|| ops.remove_account(&account))
        .expect("the removal lands");

    assert!(
        credentials.stored("work").is_none(),
        "the credential entry goes"
    );
    assert_eq!(
        dials.ids(),
        vec!["home".to_owned()],
        "the dial table drops the account"
    );
    assert_eq!(
        dials.indexed_ids(),
        vec!["home".to_owned()],
        "the index file drops the account"
    );

    // An id nothing holds still answers Ok: the desired end state.
    let ghost = AccountId::try_from("ghost").expect("an account id");
    runtime
        .block_on(|| ops.remove_account(&ghost))
        .expect("removing nothing is the end state already");
}

#[test]
fn a_refused_removal_keeps_the_table_entry() {
    let dials = table("remove-refused");
    let ops = HostAccountStore::new(
        Arc::clone(&dials),
        FakeStore::refusing(),
        Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
    );
    let account = work();

    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| ops.remove_account(&account))
        .expect_err("the refused delete refuses the removal");

    assert_eq!(
        dials.ids(),
        vec!["home".to_owned(), "work".to_owned()],
        "nothing goes when the seam refuses"
    );
}
