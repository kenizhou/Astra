// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the outage state: the streak rules (two network-class
//! failures mark the outage, any success clears, a permanent failure never
//! counts and clears a standing streak), the banner's every-dialled rule,
//! the report classification the typed call folds from, and the wiring that
//! records every ordinary pass's outcome.

use std::sync::{Arc, Mutex};

use engine_api::{AccountId, FolderSync, MailSyncReport, SyncError, SyncScope};

use crate::dispatch_fakes as fakes;
use crate::outage::PassOutcome;
use crate::{App, Intent};

/// One account's id.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// An app over two healthy accounts, the banner tests' standing start.
fn two_accounts() -> App<fakes::FakeProvider> {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    fakes::app(
        vec![
            fakes::account("work", fakes::FakeProvider::with(vec![fakes::inbox()])),
            fakes::account("home", fakes::FakeProvider::with(vec![fakes::inbox()])),
        ],
        &surfaces,
    )
}

/// A report whose every step failed with `error`.
fn failed_pass(error: SyncError) -> MailSyncReport {
    MailSyncReport {
        account_steps: Ok(()),
        mailboxes: None,
        folders: vec![FolderSync {
            scope: SyncScope::JmapType {
                account: account("work"),
                data_type: engine_core::sync::JmapDataType::Email,
            },
            result: Err(error),
            elapsed: std::time::Duration::ZERO,
            timing: engine_api::SyncTiming::default(),
        }],
        elapsed: std::time::Duration::ZERO,
    }
}

#[test]
fn a_retryable_provider_failure_is_network_class() {
    assert_eq!(
        PassOutcome::of(&failed_pass(SyncError::Provider(
            engine_provider::ProviderError::retryable("no route to host")
        ))),
        PassOutcome::NetworkFailure,
        "the class the outage counts"
    );
}

#[test]
fn a_permanent_provider_failure_is_not_network_class() {
    assert_eq!(
        PassOutcome::of(&failed_pass(SyncError::Provider(
            engine_provider::ProviderError::permanent("relay access denied")
        ))),
        PassOutcome::PermanentFailure,
    );
}

#[test]
fn a_local_fault_is_a_success_for_outage_purposes() {
    // A decode failure never asked the network anything; the store is the
    // problem, and a broken store is not an offline server.
    assert_eq!(
        PassOutcome::of(&failed_pass(SyncError::Decode("a corrupt row".to_owned()))),
        PassOutcome::Success,
    );
    assert_eq!(
        PassOutcome::of(&MailSyncReport {
            account_steps: Ok(()),
            mailboxes: None,
            folders: Vec::new(),
            elapsed: std::time::Duration::ZERO,
        }),
        PassOutcome::Success,
        "a pass with nothing wrong is a success"
    );
}

#[tokio::test]
async fn one_network_failure_is_not_yet_an_outage() {
    let app = two_accounts();
    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    assert!(!app.account_in_outage(&account("work")));
    assert!(!app.offline_banner());
}

#[tokio::test]
async fn two_network_failures_put_the_account_in_outage() {
    let app = two_accounts();
    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    assert!(app.account_in_outage(&account("work")));
    assert!(app.offline_banner(), "the only dialled account is out");
}

#[tokio::test]
async fn any_success_clears_the_streak() {
    let app = two_accounts();
    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    app.record_pass_outcome(&account("work"), PassOutcome::Success);
    assert!(!app.account_in_outage(&account("work")));
    assert!(!app.offline_banner());
}

#[tokio::test]
async fn a_permanent_failure_never_counts_and_clears_a_standing_streak() {
    let app = two_accounts();
    for _ in 0..5 {
        app.record_pass_outcome(&account("work"), PassOutcome::PermanentFailure);
    }
    assert!(
        !app.account_in_outage(&account("work")),
        "a dead account is the accounts page's story, not the banner's"
    );

    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    assert!(app.account_in_outage(&account("work")));
    app.record_pass_outcome(&account("work"), PassOutcome::PermanentFailure);
    assert!(
        !app.account_in_outage(&account("work")),
        "the newest word is dead, not offline"
    );
}

#[tokio::test]
async fn the_banner_needs_every_dialled_account_out() {
    let app = two_accounts();
    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    app.record_pass_outcome(&account("home"), PassOutcome::Success);
    assert!(app.account_in_outage(&account("work")));
    assert!(
        !app.offline_banner(),
        "one standing account keeps the banner off"
    );

    app.record_pass_outcome(&account("home"), PassOutcome::NetworkFailure);
    app.record_pass_outcome(&account("home"), PassOutcome::NetworkFailure);
    assert!(app.offline_banner(), "every dialled account is out");
}

#[test]
fn an_undialled_room_shows_no_banner() {
    let app = two_accounts();
    assert!(!app.offline_banner(), "nothing was dialled");
}

#[tokio::test]
async fn an_ordinary_pass_records_its_outcome_and_clears_a_standing_outage() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(
        vec![fakes::account(
            "work",
            fakes::FakeProvider::with_mail(vec![fakes::inbox()], Vec::new()),
        )],
        &surfaces,
    );
    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    app.record_pass_outcome(&account("work"), PassOutcome::NetworkFailure);
    assert!(app.offline_banner());

    // A healthy pass through the app's own path records its success, the
    // same fold the background loop's outcomes join.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(!app.offline_banner(), "the pass that landed cleared it");
}
