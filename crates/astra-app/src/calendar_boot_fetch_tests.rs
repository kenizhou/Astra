// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the boot calendar fetch: the first-sync join fills the
//! account's diary in the store without firing the calendar surface signal
//! (the paint half is the boot prime; the pane's own open refresh is what
//! republishes), an account with no calendar leg syncs its mail alone, and
//! a calendar the server will not serve never takes the mail sync down
//! with it.

use std::sync::{Arc, Mutex};

use engine_api::{AccountId, Horizon};

use crate::calendar_fakes::{account, calendar_app, utc, weekly_event, CalendarFake};
use crate::Surface;

/// The work account's id.
fn work() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// The standup's weekly series, the diary shape the fetch lands.
fn standup() -> engine_api::Event {
    weekly_event("evt-1", "Standup", "2026-10-19T09:00:00")
}

/// A window wide enough to hold the fixture's occurrences wherever the
/// seeded expansion window landed around the running now.
fn wide_window() -> Horizon {
    Horizon::new(utc("2026-01-01T00:00:00Z"), utc("2099-01-01T00:00:00Z")).unwrap()
}

#[tokio::test]
async fn the_first_sync_fetches_the_calendar_without_signalling_the_surface() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail().holding(standup());
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    // The boot's paint half: the prime publishes the persisted diary before
    // any sync runs, so the fetch's silence is not the empty app's silence.
    app.prime_snapshot().await;
    surfaces.lock().unwrap().clear();

    app.sync_added_account(&work()).await;

    let diary = app.engine.events(&work()).await.expect("the store reads");
    assert_eq!(
        diary.len(),
        1,
        "the first sync fetches the account's calendar beside its mail"
    );
    let occurrences = app
        .engine
        .occurrences_in(&work(), wide_window())
        .await
        .expect("the occurrence read");
    assert!(
        !occurrences.is_empty(),
        "the fetched series is expanded into occurrences"
    );
    assert!(
        !surfaces.lock().unwrap().contains(&Surface::Calendar),
        "the boot fetch never signals the calendar surface: a boot-time \
         intent would republish it needlessly"
    );
}

#[tokio::test]
async fn an_account_without_a_calendar_leg_syncs_its_mail_alone() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .holding(standup())
        .mail_only();
    let app = calendar_app(vec![account("work", fake)], &surfaces);

    app.sync_added_account(&work()).await;

    let diary = app.engine.events(&work()).await.expect("the store reads");
    assert!(diary.is_empty(), "a mail-only account fetches no calendar");
    assert!(
        !surfaces.lock().unwrap().contains(&Surface::Calendar),
        "nothing calendar-shaped fires for an account with no calendar leg"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::MailboxList),
        "the mail half of the join still rebuilds the list"
    );
}

#[tokio::test]
async fn a_calendar_the_server_will_not_serve_never_fails_the_mail_sync() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .holding(standup())
        .failing_syncs(1);
    let app = calendar_app(vec![account("work", fake)], &surfaces);

    app.sync_added_account(&work()).await;

    assert!(
        surfaces.lock().unwrap().contains(&Surface::MailboxList),
        "the mail sync lands whatever the calendar fetch did"
    );
    assert!(
        !surfaces.lock().unwrap().contains(&Surface::Calendar),
        "a failed fetch publishes no calendar"
    );
}
