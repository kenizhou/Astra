// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the idle autosave, on paused tokio time so thirty seconds of idleness
//! costs nothing and the "not before the idle" half of the contract is a clock reading
//! rather than a sleep the suite would flake on. A child of the drafts tests, reusing
//! their fixtures; its own file to keep each test module under the 500-line limit.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use astra_viewmodel::DraftSaveState;

use super::{app_over, fakes, save};
use crate::{App, Surface};

/// Spawns the autosave driver on the test's runtime, the way boot hands it to the
/// host's.
fn drive(app: &Arc<App<fakes::FakeProvider>>) {
    let driver = Arc::clone(app);
    tokio::spawn(async move {
        driver.drive_draft_autosave().await;
    });
}

#[tokio::test(start_paused = true)]
async fn the_autosave_fires_after_the_idle_and_not_before() {
    let provider = fakes::FakeProvider::saving_nothing_for(1);
    let puts = provider.draft_puts();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, Duration::from_secs(30), &surfaces);
    drive(&app);

    app.dispatch(save("compose-1", "on a train")).await.unwrap();
    assert!(puts.lock().unwrap().is_empty(), "the save had no network");
    assert_eq!(
        app.draft_status().map(|status| status.state),
        Some(DraftSaveState::Saving),
        "a save the outbox is holding is still on its way"
    );

    tokio::time::sleep(Duration::from_secs(29)).await;
    assert!(
        puts.lock().unwrap().is_empty(),
        "the idle has not passed, so nothing has fired"
    );

    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        puts.lock().unwrap().len(),
        1,
        "the idle fire retried the save and this time it landed"
    );
    assert_eq!(
        app.draft_status().map(|status| status.state),
        Some(DraftSaveState::Saved)
    );
}

#[tokio::test(start_paused = true)]
async fn a_new_save_re_arms_the_idle_from_its_own_arrival() {
    let provider = fakes::FakeProvider::saving_nothing_for(2);
    let puts = provider.draft_puts();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, Duration::from_secs(30), &surfaces);
    drive(&app);

    app.dispatch(save("compose-1", "still writing"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(25)).await;
    // The re-save restarts the interval: the trigger is the pause, not the clock, so
    // someone still typing is never made to wait on an upload.
    app.dispatch(save("compose-1", "still writing"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(29)).await;
    assert!(
        puts.lock().unwrap().is_empty(),
        "only 29 of the 30 seconds since the last save have passed"
    );

    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        puts.lock().unwrap().len(),
        1,
        "the fire comes 30 seconds after the last save, not after the first"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::DraftStatus),
        "the fire's landing is signalled"
    );
}

#[tokio::test(start_paused = true)]
async fn a_discard_cancels_the_armed_autosave() {
    let provider = fakes::FakeProvider::saving_nothing_for(u32::MAX);
    let puts = provider.draft_puts();
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(provider, Duration::from_secs(30), &surfaces);
    drive(&app);

    app.dispatch(save("compose-1", "never mind")).await.unwrap();
    app.dispatch(crate::Intent::Drafts(crate::DraftsIntent::Discard {
        composition: super::composition("compose-1"),
    }))
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert!(
        puts.lock().unwrap().is_empty(),
        "a discarded composition must not store anything after it is gone"
    );
}
