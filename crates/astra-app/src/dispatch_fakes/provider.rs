// SPDX-License-Identifier: GPL-3.0-only
//! The provider legs of the fake: the capability answer, the folder-list and
//! email sync passes, the source, edit, report, draft, and submit overrides.
//! Split from the fixture root so that file stays under the 500-line limit; a
//! trait impl cannot split across modules, so the whole [`Provider`] block
//! lives here beside the fixture root's struct and builders.

use std::sync::atomic::Ordering;

use async_trait::async_trait;
use engine_api::{
    AccountId, Capabilities, Draft, Mailbox, Message, Provider, ProviderKey, SubmissionReceipt,
    SyncWindow,
};
use engine_core::raw::RawMime;
use engine_core::sync::{SyncState, SyncUpdate};
use engine_provider::{
    ConnectionInfo, EmailChunk, EmailStream, HandOver, MailEdit, MailEditReceipt, MessageReport,
    ProviderError, ProviderResult, ReportReceipt, ScopeSync,
};

use super::{FakeProvider, ScriptedSend};

#[async_trait]
impl Provider for FakeProvider {
    fn connection_info(&self) -> ConnectionInfo {
        let mut capabilities = Capabilities::none()
            .with_mail()
            .with_mail_drafts()
            .with_submission();
        if self.tree_writes {
            capabilities = capabilities.with_mailbox_writes();
        }
        if let Some(controls) = self.report_controls {
            capabilities = capabilities.with_mail_report(controls);
        }
        ConnectionInfo::new(capabilities)
    }

    async fn sync_mailboxes(
        &self,
        _account: &AccountId,
        _cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Mailbox>> {
        // The whole tree on every pass, the IMAP shape (a LIST): a tree the
        // writes moved is presented as it stands, so a post-edit reconcile
        // lands the new folders.
        let mailboxes = self.mailboxes.lock().unwrap().clone();
        let present = mailboxes
            .iter()
            .map(|mailbox| mailbox.id.key().clone())
            .collect();
        Ok(ScopeSync::new(
            SyncUpdate::snapshot(mailboxes, present),
            SyncState::new("mbox-1"),
        ))
    }

    fn stream_email<'a>(
        &'a self,
        _account: &'a AccountId,
        cursor: Option<&'a SyncState>,
        _window: SyncWindow,
        _fetch_batch: usize,
        _chunk_size: usize,
    ) -> EmailStream<'a> {
        self.syncs.fetch_add(1, Ordering::SeqCst);
        let state = if cursor.is_some() {
            "email-2"
        } else {
            "email-1"
        };
        let messages = self.messages.lock().unwrap().clone();
        // The first pass delivers the whole mailbox; a later pass delivers whatever
        // the test queued as a delta and re-presents the keys it still holds with
        // nothing else changed, so a reconcile never tombstones what a local edit
        // moved (the store shows queued keyword changes until the server answers).
        let changed = if cursor.is_some() {
            self.updates.lock().unwrap().clone()
        } else {
            messages.clone()
        };
        let present = messages
            .iter()
            .map(|message| message.id.key().clone())
            .collect();
        Box::pin(futures::stream::iter(vec![Ok(EmailChunk::reconcile_last(
            changed,
            present,
            Some(messages.len()),
            SyncState::new(state),
        ))]))
    }

    async fn fetch_message_source(
        &self,
        _account: &AccountId,
        message: &Message,
    ) -> ProviderResult<RawMime> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        self.sources
            .lock()
            .unwrap()
            .get(message.id.key().as_str())
            .cloned()
            .map(RawMime::new)
            .ok_or_else(|| ProviderError::invalid_state("the fake holds no source for that key"))
    }

    async fn edit_mail(
        &self,
        _account: &AccountId,
        edit: &MailEdit,
    ) -> ProviderResult<MailEditReceipt> {
        self.edits.lock().unwrap().push(edit.clone());
        Ok(MailEditReceipt::new(edit.target().clone()))
    }

    /// Records one report and answers its receipt, the way a server that
    /// trains on the keyword would: the report's verdict (the keyword) beside
    /// its filing destination is the whole witness, so the report assertions
    /// read the log the same way the edit assertions read theirs.
    async fn report_message(
        &self,
        _account: &AccountId,
        report: &MessageReport,
    ) -> ProviderResult<ReportReceipt> {
        self.reports.lock().unwrap().push(report.clone());
        Ok(ReportReceipt::new(report.target.clone()))
    }

    /// Stores a draft, answering with a key that **moves on every save**, which is what
    /// three of the four real adapters do. A fake echoing the key back would let a caller
    /// that never passes `replacing` pass this suite and store a second copy per save
    /// against a real server. The stored copy joins the server's mailbox, so a later sync
    /// pass presents it to the store and a resume can read it back.
    async fn put_draft(
        &self,
        _account: &AccountId,
        draft: &Draft,
        replacing: Option<&ProviderKey>,
    ) -> ProviderResult<ProviderKey> {
        {
            let mut offline = self.offline_saves.lock().unwrap();
            if *offline > 0 {
                *offline -= 1;
                return Err(ProviderError::retryable("no route to host"));
            }
        }
        let key = {
            let mut puts = self.draft_puts.lock().unwrap();
            puts.push((draft.clone(), replacing.cloned()));
            ProviderKey::new(format!("draft-{}", puts.len())).unwrap()
        };
        self.store_draft(key.as_str(), draft);
        Ok(key)
    }

    /// Removes a stored draft from the server's mailbox along with its source, the way a
    /// real Drafts folder would, so a later pass reconciles it away.
    async fn delete_draft(&self, _account: &AccountId, draft: &ProviderKey) -> ProviderResult<()> {
        self.draft_deletes.lock().unwrap().push(draft.clone());
        self.messages
            .lock()
            .unwrap()
            .retain(|message| message.id.key() != draft);
        self.updates
            .lock()
            .unwrap()
            .retain(|message| message.id.key() != draft);
        self.sources.lock().unwrap().remove(draft.as_str());
        Ok(())
    }

    /// Delivers a message over the scripted SMTP leg: the one attempt shape
    /// whose every honest outcome the outbox exists to hold. The hand-over is
    /// committed only where a real transport would have crossed its point of
    /// no return (the message went out, the answer is what was lost), so a
    /// recovery reading the record draws the same conclusion it would against
    /// a live server.
    async fn submit_email(
        &self,
        _account: &AccountId,
        draft: &Draft,
        hand_over: &HandOver<'_>,
    ) -> ProviderResult<SubmissionReceipt> {
        let attempt = self.submits.lock().unwrap().len() + 1;
        self.submits.lock().unwrap().push(draft.clone());
        let answer = self
            .send_script
            .lock()
            .unwrap()
            .get(attempt - 1)
            .copied()
            .unwrap_or(ScriptedSend::Delivered);
        match answer {
            ScriptedSend::Delivered => {
                let handed_over = hand_over.commit().await?;
                let key = ProviderKey::new(format!("sent-{attempt}")).unwrap();
                let copy = super::sent_copy(key.as_str(), draft);
                self.messages.lock().unwrap().push(copy.clone());
                self.updates.lock().unwrap().push(copy);
                Ok(SubmissionReceipt::filed(
                    key,
                    draft.message_id.clone(),
                    &handed_over,
                ))
            }
            // No connection, so no byte left the device: no hand-over record.
            ScriptedSend::Unreachable => Err(ProviderError::retryable("no route to host")),
            // The dialogue answered no before any data was sent: nothing was
            // delivered, and no hand-over was crossed.
            ScriptedSend::Refused => Err(ProviderError::permanent("relay access denied")),
            // The message was handed to the transport and the connection died
            // before the acknowledgement: the record must say it may have gone.
            ScriptedSend::Silent => {
                hand_over.commit().await?;
                Err(ProviderError::needs_confirmation(
                    "the server took the message but did not answer",
                ))
            }
        }
    }
}
