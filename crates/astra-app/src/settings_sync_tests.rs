// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the per-account sync settings: the intent persists and the
//! settings row echoes the effective values, the defaults an unset account
//! falls back to are exactly the standing constants, the cadence clamps,
//! the depth threads the pass window (proved through a provider that
//! records the window it was handed), and the search depth note reads the
//! same setting.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use astra_viewmodel::{AccountSyncSettings, SyncBodyCap};
use async_trait::async_trait;
use engine_api::{AccountId, Capabilities, Mailbox, ProviderKey, SyncWindow};
use engine_core::sync::{SyncState, SyncUpdate};
use engine_provider::{ConnectionInfo, EmailStream, Provider, ProviderResult, ScopeSync};

use crate::accounts::NEW_ACCOUNT_SYNC_DEPTH_MONTHS;
use crate::dispatch_fakes as fakes;
use crate::preferences::{DEFAULT_BODY_CAP_OCTETS, DEFAULT_POLL_MINUTES};
use crate::protocol::SettingsIntent;
use crate::{Account, App, Intent};

/// The work account's id.
fn work() -> AccountId {
    AccountId::try_from("work").unwrap()
}

/// An app over one work account, its preferences under a scratch path of
/// the test's own.
fn app(scratch: &str) -> (App<fakes::FakeProvider>, std::path::PathBuf) {
    let dir = fakes::scratch(scratch);
    let path = dir.join("preferences.toml");
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    (
        fakes::app_with_prefs(
            vec![fakes::account(
                "work",
                fakes::FakeProvider::with(vec![fakes::inbox()]),
            )],
            Some(path.clone()),
            &surfaces,
        ),
        path,
    )
}

/// Sets the account's sync settings; each `None` field crosses no key, the
/// omission discipline the arm's serde skips encode.
async fn set<P: engine_api::Provider>(
    app: &App<P>,
    depth_months: Option<u16>,
    body_cap: Option<SyncBodyCap>,
    poll_minutes: Option<u16>,
) {
    app.dispatch(Intent::Settings(SettingsIntent::SetSyncSettings {
        account: work(),
        depth_months,
        body_cap,
        poll_minutes,
    }))
    .await
    .unwrap();
}

/// The settings row the snapshot carries for the work account.
fn row(app: &App<fakes::FakeProvider>) -> AccountSyncSettings {
    app.settings()
        .accounts
        .iter()
        .find(|row| row.account == work())
        .and_then(|row| row.sync)
        .expect("the row carries its sync settings")
}

/// A fresh app over the same preferences file, the relaunch the persistence
/// test reads through.
async fn relaunched(path: std::path::PathBuf) -> App<fakes::FakeProvider> {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = fakes::app_with_prefs(
        vec![fakes::account(
            "work",
            fakes::FakeProvider::with(vec![fakes::inbox()]),
        )],
        Some(path),
        &surfaces,
    );
    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();
    app
}

#[tokio::test]
async fn an_unset_account_runs_the_standing_constants() {
    let (app, _path) = app("defaults");
    app.dispatch(Intent::Settings(SettingsIntent::Refresh))
        .await
        .unwrap();
    let sync = row(&app);
    assert_eq!(sync.depth_months, NEW_ACCOUNT_SYNC_DEPTH_MONTHS);
    assert_eq!(
        sync.body_cap,
        SyncBodyCap::Bytes {
            octets: DEFAULT_BODY_CAP_OCTETS
        }
    );
    assert_eq!(sync.poll_minutes, DEFAULT_POLL_MINUTES);
}

#[tokio::test]
async fn the_settings_persist_echo_and_survive_a_relaunch() {
    let (app, path) = app("persist");
    set(
        &app,
        Some(12),
        Some(SyncBodyCap::Bytes { octets: 1024 }),
        Some(90),
    )
    .await;
    let stood = AccountSyncSettings {
        depth_months: 12,
        body_cap: SyncBodyCap::Bytes { octets: 1024 },
        poll_minutes: 90,
    };
    assert_eq!(row(&app), stood);
    assert_eq!(row(&relaunched(path).await), stood);
}

#[tokio::test]
async fn an_absent_field_keeps_what_stood() {
    let (app, _path) = app("merge");
    set(&app, Some(12), None, None).await;
    set(&app, None, Some(SyncBodyCap::Everything), None).await;
    assert_eq!(
        row(&app),
        AccountSyncSettings {
            depth_months: 12,
            body_cap: SyncBodyCap::Everything,
            poll_minutes: DEFAULT_POLL_MINUTES,
        },
        "each edit moves only the field it named"
    );
}

#[tokio::test]
async fn the_cadence_clamps_into_its_bounds() {
    let (app, _path) = app("clamp");
    set(&app, None, None, Some(5)).await;
    assert_eq!(row(&app).poll_minutes, 15, "the floor");
    set(&app, None, None, Some(999)).await;
    assert_eq!(row(&app).poll_minutes, 120, "the ceiling");
}

/// A provider that records the sync window each pass hands it and holds no
/// mail, so the per-account depth's effect on the pass is observable
/// without a server.
struct WindowProbe {
    windows: Mutex<Vec<SyncWindow>>,
}

#[async_trait]
impl Provider for WindowProbe {
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::new(Capabilities::none().with_mail())
    }

    async fn sync_mailboxes(
        &self,
        _account: &AccountId,
        _cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Mailbox>> {
        Ok(ScopeSync::new(
            SyncUpdate::snapshot(Vec::new(), BTreeSet::<ProviderKey>::new()),
            SyncState::new("probe-1"),
        ))
    }

    fn stream_email<'a>(
        &'a self,
        _account: &'a AccountId,
        _cursor: Option<&'a SyncState>,
        window: SyncWindow,
        _fetch_batch: usize,
        _chunk_size: usize,
    ) -> EmailStream<'a> {
        self.windows.lock().unwrap().push(window);
        Box::pin(futures::stream::iter(Vec::new()))
    }
}

impl engine_provider::MailboxWrites for WindowProbe {}
impl engine_provider::CalendarWrites for WindowProbe {}

/// The windows the account's passes have handed the probe so far.
async fn probe_windows(app: &App<WindowProbe>) -> Vec<SyncWindow> {
    app.account_handle(&work())
        .await
        .expect("the account stands")
        .providers[0]
        .windows
        .lock()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn the_depth_threads_the_pass_window() {
    let app = App::new(
        Arc::new(engine_api::Engine::open_in_memory().unwrap()),
        vec![Account {
            id: work(),
            providers: vec![WindowProbe {
                windows: Mutex::new(Vec::new()),
            }],
            identity: engine_api::EmailAddress::new("me@work.local"),
        }],
        None,
        Arc::new(fakes::RecordingObserver {
            surfaces: Arc::new(Mutex::new(Vec::new())),
        }),
    )
    .with_prefs_path(None);

    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        probe_windows(&app).await.iter().any(SyncWindow::is_bounded),
        "the product default depth bounds the pass"
    );

    set(&app, Some(0), None, None).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        probe_windows(&app)
            .await
            .iter()
            .any(|window| !window.is_bounded()),
        "a depth of zero passes the whole mailbox"
    );
}

#[tokio::test]
async fn the_search_depth_note_reads_the_same_setting() {
    let (app, _path) = app("search-note");
    set(&app, Some(12), None, None).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::Search {
        query: Some("monitor".to_owned()),
    })
    .await
    .unwrap();
    let state = app.mailbox_list().search.expect("the search publishes");
    assert_eq!(state.depth_months, 12, "the note reads the setting");
}
