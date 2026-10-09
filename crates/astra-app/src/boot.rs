// SPDX-License-Identifier: GPL-3.0-only
//! The boot sequence: open the engine, put its interrupted sync leases and outbox ops
//! back, build the app, prime the snapshot from the persisted store, and hand each
//! account's first sync to the host's runtime.
//!
//! A plain async function over the host-provided pieces rather than a host-side
//! builder: the sequence is the product core's (the ordering of lease recovery before
//! any sync, and of priming before any spawn, is what the cached-first-paint contract
//! rests on), while the runtime it spawns on stays the host's. The host passes a
//! `spawn` closure over its own executor; nothing here builds or names one.

use std::{future::Future, path::Path, pin::Pin, sync::Arc};

use engine_api::{Engine, Provider};
use thiserror::Error;

use crate::{preferences_path, Account, App, AppObserver, MailboxConnector};

/// A boxed task the host runs on its own runtime: the first sync of one account.
pub type SpawnedTask = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Why a boot failed. Only the failures that prevent *any* account from working rise
/// this far; the app opens with its cached state whatever a single account's sync later
/// does.
#[derive(Debug, Error)]
pub enum BootError {
    /// The engine store could not be opened or migrated.
    #[error("engine error: {0}")]
    Engine(String),
    /// Interrupted sync leases could not be abandoned.
    #[error("engine error: {0}")]
    Leases(String),
}

/// Boots the app over the engine store at `engine_path`.
///
/// The accounts arrive host-connected (the host dials them: the login blocks, and the
/// TLS trust policy and credentials live there), the observer is the notification path
/// every [`Surfaced`](crate::Surfaced) publish signals, and `spawn` runs each account's
/// first sync on the host's runtime: priming paints the cached list first, and the
/// first sync is the longest wait the app asks for.
///
/// The preferences file sits beside the engine store, in the same directory.
///
/// # Errors
///
/// Returns [`BootError`] when the engine cannot open its store or abandon the leases a
/// previous run left; nothing else stops a boot.
pub async fn boot<P, S>(
    engine_path: &Path,
    accounts: Vec<Account<P>>,
    connector: Option<Box<dyn MailboxConnector<P>>>,
    observer: Arc<dyn AppObserver>,
    spawn: S,
) -> Result<Arc<App<P>>, BootError>
where
    P: Provider + 'static,
    S: Fn(SpawnedTask),
{
    let engine = Engine::open(engine_path).map_err(|err| BootError::Engine(err.to_string()))?;
    let engine = Arc::new(engine);
    let abandoned = engine
        .abandon_sync_leases()
        .await
        .map_err(|err| BootError::Leases(err.to_string()))?;
    if abandoned > 0 {
        log::warn!("boot: abandoned {abandoned} interrupted sync scope lease(s) the last run left");
    }
    let prefs_path = engine_path
        .parent()
        .map_or_else(|| preferences_path(Path::new(".")), preferences_path);
    let app = App::new(engine, accounts, connector, observer).with_prefs_path(Some(prefs_path));
    let app = Arc::new(app);
    // Before anything can drain: a write the last run left mid-attempt is put back the
    // way the point it reached allows, so the first drain sends what never left and
    // nothing sends what may have.
    app.recover_outbox().await;
    // Render the persisted store's pane before returning, so the host paints the cached
    // tree the instant it comes up instead of a blank pane while the first sync runs.
    app.prime_snapshot().await;
    // Each account's first sync runs on the host's runtime, off the boot path: priming
    // has already put the cached list on screen, and the sync republishes when it lands.
    for id in app.account_ids().await {
        let app = Arc::clone(&app);
        spawn(Box::pin(async move { app.sync_added_account(&id).await }));
    }
    // The drafts idle-autosave driver runs for the life of the app, on the same host
    // seam: it sleeps until a composer has been quiet long enough to save, so it must
    // never sit on the boot path.
    {
        let app = Arc::clone(&app);
        spawn(Box::pin(async move {
            app.drive_draft_autosave().await;
        }));
    }
    Ok(app)
}
