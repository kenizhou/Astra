// SPDX-License-Identifier: GPL-3.0-only
//! Per-account sync of one account's folders. [`sync_account_providers`] hands the
//! account's providers to the engine, which syncs the folder list once and fans the
//! folders out itself, then binds and syncs any folder the list names that no provider
//! is bound to yet, and writes the pass to the log. A free function (not a method) so
//! several accounts can run it at once under one `join_all`.

use std::collections::BTreeSet;

use astra_account::pass_syncs;
use engine_api::{
    AccountId, Engine, IgnoreCommits, MailSyncReport, Mailbox, MailboxId, MailboxRole, Provider,
    StreamTuning, SyncScope,
};
use futures::future::join_all;

use crate::{Account, MailboxConnector};

/// The folder a mail scope is bound to, or `None` for a scope that covers the whole
/// account (JMAP, Gmail).
fn scope_folder(scope: &SyncScope) -> Option<&MailboxId> {
    match scope {
        SyncScope::ImapMailbox { mailbox, .. }
        | SyncScope::GraphFolder {
            folder: mailbox, ..
        } => Some(mailbox),
        _ => None,
    }
}

/// The folders `providers` are bound to, or `None` when one of them covers the whole
/// account and so every folder is bound already.
fn bound_folders<P: Provider>(account: &AccountId, providers: &[P]) -> Option<BTreeSet<MailboxId>> {
    providers
        .iter()
        .map(|provider| scope_folder(&provider.email_scope(account)).cloned())
        .collect()
}

/// Whether the folder `scope` is bound to is still in the account's folder list. A
/// scope that names no folder covers the account and is always listed, and so is the
/// `INBOX`, which IMAP binds by its reserved name whatever the listed row calls it.
fn still_listed(listed: &[Mailbox], scope: &SyncScope) -> bool {
    scope_folder(scope).is_none_or(|folder| {
        folder.as_str().eq_ignore_ascii_case("INBOX")
            || listed.iter().any(|mailbox| &mailbox.id == folder)
    })
}

/// Syncs one account's mail: the folder list **once**, then every folder's email with
/// the engine driving the fan-out (distinct per-folder scopes never contend). A folder
/// the list names that no provider is bound to is bound through `connector` and synced
/// in the same pass, so every folder the account lists is synced by every pass.
///
/// Reports nothing but the log: the per-scope outcomes that drive reachability badges,
/// sign-in prompts, and throttle waits arrive with the connectivity surface they answer
/// to, and this milestone has none of them.
pub(crate) async fn sync_account_providers<P: Provider>(
    engine: &Engine,
    account: &Account<P>,
    connector: Option<&dyn MailboxConnector<P>>,
    tuning: StreamTuning,
    acct: usize,
) {
    // `acct` is the account's stable position in the pane (not its id, which carries
    // the address) so the interleaved timing of concurrent accounts stays attributable
    // without logging identity.
    if account.providers.is_empty() {
        log::info!("sync[a{acct}]: skipped; no live mail providers");
        return;
    }

    let mut report = engine
        .sync_mail(&account.providers, &account.id, tuning, &IgnoreCommits)
        .await;
    // The folder list as this pass left it, when the pass read one: where the folders
    // no provider is bound to are found, and what tells a folder the server no longer
    // has from one that failed.
    let listed = match &report.mailboxes {
        Some(Ok(_)) => engine.mailboxes(&account.id).await.ok(),
        _ => None,
    };
    if let Some(listed) = &listed {
        sync_unbound_folders(engine, account, listed, connector, tuning, &mut report).await;
    }
    log_pass(acct, &report, listed.as_deref());
}

/// Syncs the folders in `listed` that `account` has no provider bound to, and folds
/// what that did into `report`.
///
/// A no-op for an account whose provider covers every folder (JMAP, Gmail), which has
/// nothing unbound, and without a connector (the tests, an unpersisted build).
async fn sync_unbound_folders<P: Provider>(
    engine: &Engine,
    account: &Account<P>,
    listed: &[Mailbox],
    connector: Option<&dyn MailboxConnector<P>>,
    tuning: StreamTuning,
    report: &mut MailSyncReport,
) {
    let Some(connector) = connector else {
        return;
    };
    let Some(bound) = bound_folders(&account.id, &account.providers) else {
        return;
    };
    let unbound: Vec<&MailboxId> = listed
        .iter()
        // The Inbox is bound by its reserved name, which the listed row need not spell
        // the same way; it is bound whatever the row says.
        .filter(|mailbox| {
            mailbox.role != Some(MailboxRole::Inbox)
                && pass_syncs(mailbox)
                && !bound.contains(&mailbox.id)
        })
        .map(|mailbox| &mailbox.id)
        .collect();
    if unbound.is_empty() {
        return;
    }

    let providers: Vec<P> = join_all(
        unbound
            .iter()
            .map(|mailbox| connector.connect_folder(&account.id, mailbox.key().as_str())),
    )
    .await
    .into_iter()
    .flatten()
    .collect();
    log::debug!(
        "sync: {} folder(s) listed since the account connected, {} bound",
        unbound.len(),
        providers.len(),
    );
    if providers.is_empty() {
        return;
    }
    let added = engine
        .refresh_folders(&providers, &account.id, tuning, &IgnoreCommits)
        .await;
    report.folders.extend(added.folders);
    report.elapsed += added.elapsed;
    if let Err(err) = added.account_steps {
        if report.account_steps.is_ok() {
            report.account_steps = Err(err);
        }
    }
}

/// Writes the pass to the diagnostic log: one line for the account, one per folder
/// that did something worth knowing about.
///
/// Counts, durations and scope **positions** only; never a folder name, an address or
/// an id. A folder is named by its index in the pass, which is enough to line a slow
/// or failing one up against the account summary above it without putting the user's
/// mailbox names in a file they hand to support.
fn log_pass(acct: usize, report: &MailSyncReport, listed: Option<&[Mailbox]>) {
    if let Err(err) = &report.account_steps {
        // Deliberately its own line, and deliberately not phrased as an outage: this
        // is the store failing, and reading it as "the server is down" is how a schema
        // problem sends someone to check their wifi.
        log::warn!("sync[a{acct}]: local store step failed: {err}");
    }
    if let Some(Err(err)) = &report.mailboxes {
        if !err.is_busy() {
            log::warn!("sync[a{acct}]: folder list failed: {err}");
        }
    }

    for (index, folder) in report.folders.iter().enumerate() {
        let ms = folder.elapsed.as_millis();
        let gone = listed.is_some_and(|listed| {
            !still_listed(listed, &folder.scope)
                && matches!(&folder.result, Err(err) if !err.is_busy())
        });
        match &folder.result {
            Ok(applied) if applied.upserted + applied.tombstoned > 0 => log::debug!(
                "sync[a{acct}]: folder[{index}] +{} -{} in {ms}ms",
                applied.upserted,
                applied.tombstoned,
            ),
            Ok(_) => log::debug!("sync[a{acct}]: folder[{index}] unchanged in {ms}ms"),
            Err(_) if gone => {
                log::debug!("sync[a{acct}]: folder[{index}] is no longer listed");
            }
            Err(err) if err.is_busy() => {
                log::debug!("sync[a{acct}]: folder[{index}] busy in {ms}ms");
            }
            Err(err) => log::warn!("sync[a{acct}]: folder[{index}] failed in {ms}ms: {err}"),
        }
    }

    let busy = report
        .folders
        .iter()
        .filter(|folder| matches!(&folder.result, Err(err) if err.is_busy()))
        .count();
    log::info!(
        "sync[a{acct}]: {} folder(s), {} synced, {} msg upserted, {} removed, {busy} busy in \
         {}ms",
        report.folders.len(),
        report.folders_synced(),
        report.upserted(),
        report.tombstoned(),
        report.elapsed.as_millis(),
    );
}
