// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the delivery-facing core seams: the first-pass join the host's
//! background loop opens its watch after, and the delivery state the
//! sync-progress snapshot carries beside the account positions.

use std::sync::{Arc, Mutex};

use engine_api::AccountId;

use crate::dispatch_fakes as fakes;
use crate::outage::PassOutcome;
use crate::{App, Surface};

/// The work account's id.
fn work() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// The home account's id.
fn home() -> AccountId {
    AccountId::try_from("home").unwrap()
}

/// An app over the two accounts, recording every signalled surface.
fn two_accounts() -> (App<fakes::FakeProvider>, Arc<Mutex<Vec<Surface>>>) {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(
        vec![
            fakes::account("work", fakes::FakeProvider::with(vec![fakes::inbox()])),
            fakes::account("home", fakes::FakeProvider::with(vec![fakes::inbox()])),
        ],
        &surfaces,
    );
    (app, surfaces)
}

/// How many times the sync-progress surface has been signalled so far.
fn progress_signals(surfaces: &Arc<Mutex<Vec<Surface>>>) -> usize {
    surfaces
        .lock()
        .unwrap()
        .iter()
        .filter(|surface| **surface == Surface::SyncProgress)
        .count()
}

#[tokio::test]
async fn the_first_pass_join_parks_until_the_account_s_first_pass_settles() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = Arc::new(fakes::app(
        vec![fakes::account(
            "work",
            fakes::FakeProvider::with(vec![fakes::inbox()]),
        )],
        &surfaces,
    ));
    // The waiter parks before any pass has run.
    let waiter = {
        let app = Arc::clone(&app);
        tokio::spawn(async move { app.wait_first_pass(&work()).await })
    };
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !waiter.is_finished(),
        "the join parks while no pass has run"
    );

    // The boot-shaped pass settles it, and a waiter that arrives later is
    // answered at once rather than parked.
    app.sync_added_account(&work()).await;
    tokio::time::timeout(std::time::Duration::from_secs(5), waiter)
        .await
        .expect("the join resolves once the pass settles")
        .expect("the waiter task joins");
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        app.wait_first_pass(&work()).await;
    })
    .await
    .expect("a later waiter resolves at once");
}

#[tokio::test]
async fn an_account_the_app_does_not_hold_answers_the_join_at_once() {
    let (app, _surfaces) = two_accounts();
    let stranger = AccountId::try_from("stranger").unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        app.wait_first_pass(&stranger).await;
    })
    .await
    .expect("an account with no pass to come answers at once");
}

#[tokio::test]
async fn an_account_with_no_providers_settles_its_first_pass() {
    // A registered account whose dial left it with no providers still owes
    // its driver a settle: the join must not park on a pass that will never
    // run.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app(
        vec![crate::Account {
            id: work(),
            providers: Vec::new(),
            identity: engine_api::EmailAddress::new("me@work.local"),
        }],
        &surfaces,
    );
    app.sync_added_account(&work()).await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        app.wait_first_pass(&work()).await;
    })
    .await
    .expect("the skipped pass still settles the join");
}

#[tokio::test]
async fn the_delivery_state_rides_the_progress_snapshot_and_moves_it() {
    let (app, surfaces) = two_accounts();
    app.dispatch(crate::Intent::RefreshMail).await.unwrap();
    let healthy = app.sync_progress();
    assert!(!healthy.delivery.offline_banner);
    assert!(healthy.delivery.outages.is_empty());

    // One network failure is not an outage; the second one is.
    app.record_pass_outcome(&work(), PassOutcome::NetworkFailure);
    let streaking = app.sync_progress();
    assert!(!streaking.delivery.offline_banner, "the home account is up");
    assert_eq!(streaking.delivery.outages, Vec::<AccountId>::new());

    app.record_pass_outcome(&work(), PassOutcome::NetworkFailure);
    let outage = app.sync_progress();
    assert!(
        !outage.delivery.offline_banner,
        "the home account is still up"
    );
    assert_eq!(outage.delivery.outages, vec![work()]);

    // Every dialled account in outage raises the banner, the badge list in
    // account order.
    app.record_pass_outcome(&home(), PassOutcome::NetworkFailure);
    app.record_pass_outcome(&home(), PassOutcome::NetworkFailure);
    let bannered = app.sync_progress();
    assert!(bannered.delivery.offline_banner);
    assert_eq!(bannered.delivery.outages, vec![home(), work()]);

    // Any successful pass clears its account and drops the banner with it.
    app.record_pass_outcome(&home(), PassOutcome::Success);
    let cleared = app.sync_progress();
    assert!(!cleared.delivery.offline_banner);
    assert_eq!(cleared.delivery.outages, vec![work()]);

    // Each move the surfaces can see was signalled on the progress
    // channel, so a standing surface pulls the new state without a pass
    // having to run.
    assert!(
        progress_signals(&surfaces) >= 4,
        "every outage move signalled the progress surface"
    );
}
