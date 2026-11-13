// SPDX-License-Identifier: GPL-3.0-only
//! The attachment browser's aggregation: a bounded fan-out over the engine's
//! existing seams (search has_attachment, resolve, message_attachments) that
//! flattens every scanned message's downloadable parts beside the source
//! row's metadata, newest first, with a coverage report stating how far the
//! scan looked. A command's answer, never a Surface: the browser recomputes
//! per open and holds no persistent index.
//!
//! The sweep fetches on a cache miss (the engine's only attachment seam is
//! cache-first but dials on a miss), bounded by the per-account scan cap;
//! the escalated reference deviation and the has_attachment guess caveat are
//! recorded in the attachment browser contract.

use astra_viewmodel::{
    AttachmentBrowserCoverage, AttachmentBrowserEntry, AttachmentBrowserFilter,
    AttachmentBrowserView, MessageKey,
};
use engine_api::{AccountId, MailListRow, MessageAttachment, Provider};

use crate::{list_rows, reading::resolve, reference::MessageRef, App};

/// How many messages one account's scan considers: the engine query's cap,
/// the same bound the warm pass draws its work list under. The coverage
/// report states it beside the answer, so a bounded browse never claims the
/// whole mailbox.
pub(crate) const SCAN_LIMIT: usize = 200;

/// One account's scan outcome: the flattened rows beside the counts the
/// coverage report folds across accounts.
#[derive(Default)]
struct AccountScan {
    /// The parts the scan listed, newest source message first.
    entries: Vec<AttachmentBrowserEntry>,
    /// The flagged messages the store resolved.
    scanned: u32,
    /// The messages whose parts the scan listed.
    indexed: u32,
    /// The messages whose read refused or no longer resolved.
    skipped: u32,
}

impl<P: Provider> App<P> {
    /// Browses attachments across every account: each account's
    /// has_attachment hits resolved newest first, their parts flattened
    /// beside the source metadata, narrowed by the filter. Per-message
    /// failures are best-effort skips; a per-account failure is an honest
    /// error entry in the coverage report. Recomputed per open: nothing
    /// persists between browses, and the engine's cache serves the second
    /// open of a source the first one fetched.
    pub async fn attachment_browser(
        &self,
        filter: AttachmentBrowserFilter,
    ) -> AttachmentBrowserView {
        let mut view = AttachmentBrowserView {
            coverage: AttachmentBrowserCoverage {
                scan_limit: u32::try_from(SCAN_LIMIT).unwrap_or(u32::MAX),
                ..AttachmentBrowserCoverage::default()
            },
            ..AttachmentBrowserView::default()
        };
        for handle in self.account_handles().await {
            let Some(provider) = handle.providers.first() else {
                log::warn!("attachment-browser: an account holds no provider to read through");
                view.coverage.failed_accounts.push(handle.id.clone());
                continue;
            };
            match self.scan_account(&handle.id, provider, &filter).await {
                Ok(scan) => {
                    view.entries.extend(scan.entries);
                    view.coverage.scanned = view.coverage.scanned.saturating_add(scan.scanned);
                    view.coverage.indexed = view.coverage.indexed.saturating_add(scan.indexed);
                    view.coverage.skipped = view.coverage.skipped.saturating_add(scan.skipped);
                }
                Err(err) => {
                    log::warn!("attachment-browser: an account's scan refused: {err}");
                    view.coverage.failed_accounts.push(handle.id.clone());
                }
            }
        }
        // One order across accounts: the newest source message first, undated
        // last. The instant is the fixed-width UTC form the list rows carry,
        // so its spelling orders chronologically; a stable sort keeps a
        // message's parts in the order it carries them.
        view.entries.sort_by(|a, b| b.sent_at.cmp(&a.sent_at));
        view
    }

    /// Scans one account: the has_attachment query under the cap, the hits
    /// resolved newest first, and each message's parts listed through the
    /// engine's cache-first read. The sender filter narrows before the fetch
    /// (a message the filter drops never dials); the mime filter narrows the
    /// parts a listed message contributes.
    async fn scan_account(
        &self,
        account: &AccountId,
        provider: &P,
        filter: &AttachmentBrowserFilter,
    ) -> Result<AccountScan, engine_api::ApiError> {
        let hits = self
            .engine
            .search_mail(account, "has_attachment:true", SCAN_LIMIT)
            .await?;
        let keys: Vec<_> = hits.hits.iter().map(|hit| hit.key.clone()).collect();
        let mut rows = self.engine.mail_by_keys(account, &keys).await?;
        rows.sort_by(list_rows::newest_first);
        let mut scan = AccountScan {
            scanned: u32::try_from(rows.len()).unwrap_or(u32::MAX),
            ..AccountScan::default()
        };
        for row in rows {
            if !keeps_sender(filter, &row) {
                continue;
            }
            let reference = MessageRef {
                account: account.clone(),
                key: row.mail.key.clone(),
            };
            let Some(original) = resolve(&self.engine, &reference).await else {
                scan.skipped = scan.skipped.saturating_add(1);
                log::debug!("attachment-browser: a flagged message no longer resolves");
                continue;
            };
            match self
                .engine
                .message_attachments(provider, account, &original)
                .await
            {
                Ok(parts) => {
                    scan.indexed = scan.indexed.saturating_add(1);
                    for part in parts {
                        if keeps_mime(filter, part.media_type()) {
                            scan.entries.push(entry(&row, &part));
                        }
                    }
                }
                Err(err) => {
                    scan.skipped = scan.skipped.saturating_add(1);
                    log::debug!("attachment-browser: one message's parts refused: {err}");
                }
            }
        }
        Ok(scan)
    }
}

/// Whether the sender filter keeps `row`: an absent filter admits every
/// sender, and a present one matches the bare address exactly and
/// case-insensitively (a display name is never the filter's target).
fn keeps_sender(filter: &AttachmentBrowserFilter, row: &MailListRow) -> bool {
    filter.sender.as_deref().is_none_or(|sender| {
        row.mail
            .from_addr
            .as_deref()
            .is_some_and(|address| address.eq_ignore_ascii_case(sender))
    })
}

/// Whether the mime filter keeps a part of `media_type`: an absent filter
/// admits every part, and a present one matches the type's prefix
/// case-insensitively (`image/` admits every image, `application/pdf` one
/// type).
fn keeps_mime(filter: &AttachmentBrowserFilter, media_type: &str) -> bool {
    filter.mime_prefix.as_deref().is_none_or(|prefix| {
        media_type
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    })
}

/// One part flattened beside its source row's metadata: the part identity a
/// save names, the source reference an open dispatches, and the display
/// scalars the group header draws.
fn entry(row: &MailListRow, part: &MessageAttachment) -> AttachmentBrowserEntry {
    AttachmentBrowserEntry {
        id: part.id().as_u32(),
        file_name: part.file_name().to_owned(),
        media_type: part.media_type().to_owned(),
        size: part.size(),
        source: MessageKey {
            account: row.account.clone(),
            key: row.mail.key.clone(),
        },
        subject: row.mail.subject.clone().unwrap_or_default(),
        from_name: row
            .mail
            .from_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .or(row.mail.from_addr.as_deref())
            .unwrap_or_default()
            .to_owned(),
        from_address: row.mail.from_addr.clone().unwrap_or_default(),
        sent_at: row
            .mail
            .date_utc
            .map_or_else(String::new, list_rows::format_instant),
    }
}

#[cfg(test)]
#[path = "attachment_browser_tests.rs"]
mod tests;
