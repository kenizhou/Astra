// SPDX-License-Identifier: GPL-3.0-only
//! The outbox family over the engine's queue ops, and the
//! [`Sending`](crate::Surface::Sending) surface that publishes the queue
//! itself. The submit arms that feed the queue live in `submit`; this is
//! what the outbox panel asks of the sends already in it.
//!
//! The engine owns the queue and every decision about it: what is due, whether
//! a failure parks or settles, and whether an op can be withdrawn at all. This
//! layer decides only what a person is offered, and turns each refusal into
//! something a log line can say rather than a silent no-op. The op surface is
//! the engine's own: cancel, retry now, and the confirmation pair
//! `confirm_pending_op` answers, which the engine at the pinned revision
//! exposes directly, so nothing here re-derives a queue rule.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content, its
//! recipients, or its subject; counts and ids only.

use std::collections::HashMap;

use astra_viewmodel::{queued_row, HeldSend, HeldState, SendingSnapshot};
use engine_api::{AccountId, Confirmation, Draft, OpRejection, PendingOpKind, Provider};

use crate::{drafts::now_instant, reference::QueuedRef, App, OutboxIntent};

/// One send the plugin gate is holding: the row the snapshot publishes plus
/// the built draft a retry or a Send Anyway resumes. The draft, never a
/// re-render: the composition the gate vetted is the composition that queues.
pub(crate) struct HeldEntry {
    /// The hold's id, minted when the gate took the composition.
    pub(crate) hold: String,
    /// The account the send leaves from.
    pub(crate) from: AccountId,
    /// The subject line.
    pub(crate) subject: String,
    /// The plugin whose verdict is awaited or whose refusal holds the send.
    pub(crate) plugin: String,
    /// The reason, in product words.
    pub(crate) reason: String,
    /// Where the hold stands.
    pub(crate) state: HeldState,
    /// The built draft, transformed if a transform ran.
    pub(crate) draft: Draft,
}

/// The sending state beside the queue itself: what the app adds that the store
/// does not hold.
#[derive(Default)]
pub(crate) struct Outbox {
    /// When each queued send first appeared in this app's read of the store,
    /// keyed by account and op id. The store stamps no arrival instant of its
    /// own, so the app notes the moment it first saw each op; a send already
    /// queued when the app started is unknown and reads as an empty stamp.
    arrived: HashMap<(AccountId, u64), String>,
    /// The sends the plugin gate is holding back from the queue, in hold
    /// order. Session state by design: the draft behind each is durable, and
    /// the hold rebuilds from a resend.
    held: Vec<HeldEntry>,
    /// The hold id counter: session-scoped, which is all a row answer needs.
    hold_serial: u64,
}

impl<P: Provider> App<P> {
    /// The sending snapshot, for the host's pull after a
    /// [`Surface::Sending`](crate::Surface::Sending) signal: the sends that
    /// have not gone out, across every account, or nothing outstanding.
    #[must_use]
    pub fn sending(&self) -> SendingSnapshot {
        self.sending.get()
    }

    /// Routes one [`OutboxIntent`] to its handler.
    pub(crate) async fn dispatch_outbox(&self, intent: OutboxIntent) {
        match intent {
            OutboxIntent::Show => self.rebuild_sending().await,
            OutboxIntent::Cancel { queued } => self.cancel_queued_send(&queued).await,
            OutboxIntent::SendNow { queued } => self.send_queued_now(&queued).await,
            OutboxIntent::ConfirmSent { queued } => {
                self.confirm_queued_send(&queued, Confirmation::Delivered)
                    .await;
            }
            OutboxIntent::ConfirmNotSent { queued } => {
                self.confirm_queued_send(&queued, Confirmation::NotDelivered)
                    .await;
            }
            OutboxIntent::AnswerGate { hold, send_anyway } => {
                self.answer_gate(&hold, send_anyway).await;
            }
            OutboxIntent::RetryHeld { hold } => self.retry_held(&hold).await,
            OutboxIntent::WithdrawHeld { hold } => self.withdraw_held(&hold).await,
        }
    }

    /// Takes one composition into the gate's hold and publishes the row,
    /// answering the hold's id. The gate calls this with `Checking` the
    /// moment it starts, so the awaited verdict is visible from the first
    /// second.
    pub(crate) async fn hold_send(
        &self,
        from: AccountId,
        draft: Draft,
        plugin: String,
        reason: String,
        state: HeldState,
    ) -> String {
        let hold = {
            let mut outbox = self.outbox.lock().expect("outbox mutex poisoned");
            outbox.hold_serial += 1;
            let hold = format!("hold-{}", outbox.hold_serial);
            outbox.held.push(HeldEntry {
                hold: hold.clone(),
                subject: draft.subject.clone(),
                from,
                draft,
                plugin,
                reason,
                state,
            });
            hold
        };
        self.rebuild_sending().await;
        hold
    }

    /// Moves a held send to a fresh state with the verdict's plugin and
    /// reason, republishing.
    pub(crate) async fn settle_hold(
        &self,
        hold: &str,
        plugin: String,
        reason: String,
        state: HeldState,
    ) {
        {
            let mut outbox = self.outbox.lock().expect("outbox mutex poisoned");
            if let Some(entry) = outbox.held.iter_mut().find(|entry| entry.hold == hold) {
                entry.plugin = plugin;
                entry.reason = reason;
                entry.state = state;
            }
        }
        self.rebuild_sending().await;
    }

    /// Removes a hold and answers its entry, republishing. A hold that
    /// already left (a double answer, a stale row click) answers nothing.
    pub(crate) async fn take_hold(&self, hold: &str) -> Option<HeldEntry> {
        let entry = {
            let mut outbox = self.outbox.lock().expect("outbox mutex poisoned");
            outbox
                .held
                .iter()
                .position(|entry| entry.hold == hold)
                .map(|position| outbox.held.remove(position))
        };
        if entry.is_some() {
            self.rebuild_sending().await;
        }
        entry
    }

    /// The prompt-user answer: Send Anyway queues the held composition
    /// without re-gating (the answer is never remembered); Deny drops the
    /// hold and keeps the draft unsent. Only a `NeedsAnswer` hold accepts an
    /// answer: a row that moved under the click cannot send by it.
    async fn answer_gate(&self, hold: &str, send_anyway: bool) {
        let needs_answer = self
            .outbox
            .lock()
            .expect("outbox mutex poisoned")
            .held
            .iter()
            .any(|entry| entry.hold == hold && entry.state == HeldState::NeedsAnswer);
        if !needs_answer {
            log::info!("outbox: a gate answer named a hold that is not asking");
            return;
        }
        let Some(entry) = self.take_hold(hold).await else {
            return;
        };
        if !send_anyway {
            log::info!("outbox: the user denied a prompted send; it stays a draft");
            return;
        }
        let Some(handle) = self.account_handle(&entry.from).await else {
            log::warn!("outbox: the held send's account is no longer configured");
            return;
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("outbox: the held send's account has no provider to send through");
            return;
        };
        log::info!("outbox: the user said Send Anyway; queueing without re-gating");
        let from = entry.from.clone();
        self.submit_and_drain(&from, provider, entry.draft).await;
    }

    /// Re-runs the gate over a held send: the hold drops and the submit seam
    /// takes the draft again, fresh verdict and all.
    async fn retry_held(&self, hold: &str) {
        let Some(entry) = self.take_hold(hold).await else {
            log::info!("outbox: a retry named a hold that is not held");
            return;
        };
        let Some(handle) = self.account_handle(&entry.from).await else {
            log::warn!("outbox: the held send's account is no longer configured");
            return;
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("outbox: the held send's account has no provider to send through");
            return;
        };
        log::info!("outbox: re-gating a held send");
        let from = entry.from.clone();
        self.gate_and_submit(&from, provider, entry.draft).await;
    }

    /// Drops a held send. The draft behind it stays in Drafts; nothing is
    /// ever queued by withdrawing.
    async fn withdraw_held(&self, hold: &str) {
        if self.take_hold(hold).await.is_some() {
            log::info!("outbox: a held send was withdrawn; the draft stays");
        }
    }

    /// Withdraws a queued send so it is never delivered, or dismisses one the
    /// server refused.
    ///
    /// A refusal is logged rather than swallowed, and the row stays on screen:
    /// the refusals that matter say the message may already have gone, which
    /// the snapshot the rebuild publishes keeps in front of the user.
    async fn cancel_queued_send(&self, queued: &QueuedRef) {
        let at = self.account_ordinal(&queued.account).await;
        match self
            .engine
            .cancel_pending_op(&queued.account, queued.op)
            .await
        {
            Ok(None) => {
                log::info!("outbox[a{at}]: a queued send was withdrawn; it will not be sent");
            }
            Ok(Some(refusal)) => {
                log::info!(
                    "outbox[a{at}]: a queued send could not be withdrawn: {}",
                    refusal_words(refusal)
                );
            }
            Err(err) => log::warn!("outbox[a{at}]: withdrawing a queued send failed: {err}"),
        }
        self.rebuild_sending().await;
    }

    /// Attempts a queued send now instead of waiting out its backoff, or sends
    /// again one the server refused, then drains so the attempt happens in
    /// this gesture rather than at a later pass.
    async fn send_queued_now(&self, queued: &QueuedRef) {
        let at = self.account_ordinal(&queued.account).await;
        match self
            .engine
            .retry_pending_op_now(&queued.account, queued.op)
            .await
        {
            Ok(None) => {
                log::info!("outbox[a{at}]: a queued send was asked for now");
                self.drain_account(&queued.account).await;
            }
            Ok(Some(refusal)) => log::info!(
                "outbox[a{at}]: a queued send cannot be sent now: {}",
                refusal_words(refusal)
            ),
            Err(err) => log::warn!("outbox[a{at}]: sending a queued send now failed: {err}"),
        }
        self.rebuild_sending().await;
    }

    /// Records the user's answer to a send whose delivery could not be
    /// confirmed.
    ///
    /// The store refuses an answer to a send that is not waiting for one, so a
    /// row that changed state under the click cannot be sent again through
    /// here. An answer that it did not arrive drains at once, as send now
    /// does: the one way a message that may have been delivered is ever
    /// attempted again.
    async fn confirm_queued_send(&self, queued: &QueuedRef, answer: Confirmation) {
        let at = self.account_ordinal(&queued.account).await;
        match self
            .engine
            .confirm_pending_op(&queued.account, queued.op, answer)
            .await
        {
            Ok(None) if answer == Confirmation::Delivered => log::info!(
                "outbox[a{at}]: the user says a queued send was delivered; it will not be sent again"
            ),
            Ok(None) => {
                log::info!(
                    "outbox[a{at}]: the user says a queued send was not delivered; sending it again"
                );
                self.drain_account(&queued.account).await;
            }
            Ok(Some(refusal)) => log::info!(
                "outbox[a{at}]: an answer for a queued send was refused: {}",
                refusal_words(refusal)
            ),
            Err(err) => log::warn!("outbox[a{at}]: recording the user's answer failed: {err}"),
        }
        self.rebuild_sending().await;
    }

    /// Re-reads every account's outbox and publishes the sending snapshot,
    /// but only when it moved: a gesture that changed nothing says nothing,
    /// and a host that opens the panel pulls the snapshot regardless.
    pub(crate) async fn rebuild_sending(&self) {
        let mut rows = Vec::new();
        for id in self.account_ids().await {
            let queued = self.engine.outbox(&id).await.unwrap_or_default();
            // The stamps are taken under a lock held across no await: each op
            // the read has not seen before is stamped now, and one that has
            // been seen keeps the instant it first appeared.
            let stamps: Vec<String> = {
                let mut state = self.outbox.lock().expect("outbox mutex poisoned");
                queued
                    .iter()
                    .map(|op| {
                        state
                            .arrived
                            .entry((id.clone(), op.id.get()))
                            .or_insert_with(now_instant)
                            .clone()
                    })
                    .collect()
            };
            for (op, stamp) in queued.iter().zip(stamps) {
                if let Some(row) = queued_row(&id, op, stamp) {
                    rows.push(row);
                }
            }
        }
        let held = self
            .outbox
            .lock()
            .expect("outbox mutex poisoned")
            .held
            .iter()
            .map(|entry| HeldSend {
                hold: entry.hold.clone(),
                account: entry.from.clone(),
                subject: entry.subject.clone(),
                plugin: entry.plugin.clone(),
                reason: entry.reason.clone(),
                state: entry.state,
            })
            .collect();
        let snapshot = SendingSnapshot { rows, held };
        if self.sending.get() != snapshot {
            self.sending.publish(snapshot);
        }
    }

    /// Drains every account's outbox through its first provider.
    ///
    /// A provider failure is not an error: the engine parks each op against
    /// its own retryable class for a later pass, and the queue still owes the
    /// attempt.
    pub(crate) async fn drain_outboxes(&self) {
        for id in self.account_ids().await {
            self.drain_account(&id).await;
        }
    }

    /// One account's drain pass. When a send moved, the queue is in the store
    /// and so is published here rather than waiting for the sync that follows,
    /// which a silent server can hold for as long as it likes.
    pub(crate) async fn drain_account(&self, id: &AccountId) {
        let Some(handle) = self.account_handle(id).await else {
            return;
        };
        let Some(provider) = handle.providers.first() else {
            return;
        };
        let at = self.account_ordinal(id).await;
        match self.engine.drain_outbox(provider, id).await {
            Ok(report) if report.is_idle() => {}
            Ok(report) => {
                log::info!(
                    "outbox[a{at}]: drain attempted {} queued write(s)",
                    report.attempted.len(),
                );
                if report
                    .attempted
                    .iter()
                    .any(|op| op.kind == PendingOpKind::MailSubmit)
                {
                    self.rebuild_sending().await;
                }
            }
            Err(err) => log::warn!("outbox[a{at}]: a drain pass failed: {err}"),
        }
    }
}

/// A refusal in words, for the log: product surface language, never the
/// engine's own variant names.
fn refusal_words(refusal: OpRejection) -> &'static str {
    match refusal {
        OpRejection::Unknown => "it is no longer queued",
        OpRejection::Settled => "it has already gone",
        OpRejection::InFlight => "it is being sent right now",
        OpRejection::AwaitingConfirmation => "it may already have been delivered",
        OpRejection::NotAwaitingConfirmation => "it is not waiting for an answer",
    }
}

#[cfg(test)]
#[path = "outbox_tests.rs"]
mod tests;
