// SPDX-License-Identifier: GPL-3.0-only
//! The reply-drafting driver: the arming an open message leaves behind and
//! the long-lived future that takes it, the A2 slice's runtime half. The
//! core owns no executor (the incoming-pass rule), so an open only arms:
//! [`App::drive_reply_drafting`] is the driver future the host runs for
//! the life of the app, sleeping until an open arms it. One generation per
//! message, deduped by pool presence (skipped rows included), the reading
//! turn's correlation riding each arming and a fresh one minted when an
//! arming carries none. The context collector lives here too: the original
//! through the reading resolve, the thread's earlier members through the
//! engine's thread read (bodies fetched per member), and the account's
//! style note through the profile getter, whose daily cap keeps a fresh
//! profile from spending an infer call. Beside the driver sit the host's
//! AI pulls (the chips read, the summarise action, the style note, the
//! chips dismissal), each routed through the one AI bundle.

use astra_ai::{AiStore, CorrelationId};
use astra_viewmodel::MessageKey;
use engine_api::{AccountId, MailListRow, MailboxRole, Provider, ProviderKey, ThreadId};

use super::draft::{generate_reply, DraftContext, THREAD_HISTORY_LIMIT};
use super::gate::{chips_for, Chip};
use super::style::{style_profile, STYLE_SAMPLE_LIMIT};
use super::summarise::{summarise, SummariseError};
use crate::ai_results::AiPass;
use crate::ai_rules::MailContext;
use crate::reference::MessageRef;
use crate::App;

/// How wide a newest-window the style collector draws its Sent samples
/// from: the row read is cheap and the daily cap bounds how often it runs,
/// so a window wide enough to hold the sample cap of Sent messages among
/// the newer Inbox ones costs nothing a profile build would notice.
pub const STYLE_SENT_WINDOW: usize = 200;

/// A host implements to learn a draft pool row landed: the reading pane's
/// signal to pull the message's chips again. Installed into the AI bundle
/// host-side; the default none lands rows with no signal, the store-only
/// shape the CLI reads.
pub trait DraftPoolSink: Send + Sync {
    /// One pool row landed for `message`, drafted or skipped alike: the
    /// pool's presence changed, whatever the row carries.
    fn draft_pool_landed(&self, message: MessageKey);
}

/// Failures of the host's summarise pull: the bundle's absence, a message
/// the store no longer resolves, and the summarise action's own failures
/// passed through.
#[derive(Debug, thiserror::Error)]
pub enum SummariseMessageError {
    /// The app runs no AI bundle, so nothing can be summarised.
    #[error("no ai bundle is configured")]
    NoBundle,
    /// The message does not resolve in the store, so there is no body to
    /// summarise.
    #[error("the message could not be resolved for summarising")]
    Unresolved,
    /// The summarise action itself failed; the detail is its own.
    #[error(transparent)]
    Summarise(#[from] SummariseError),
}

/// One armed reply draft: the message whose open asked for a generation,
/// beside the correlation the open minted for its reading turn.
pub(crate) struct DraftArm {
    /// The message to draft a reply for.
    pub(crate) message: MessageRef,
    /// The reading turn's correlation, when the arming site minted one;
    /// the driver mints fresh for an arming that carries none.
    pub(crate) correlation: Option<CorrelationId>,
}

/// The collected inputs one armed draft generates against: the original's
/// context, the thread's earlier members (newest first, exactly as the
/// engine's thread read returns them), and the account's style note when
/// one stands.
struct DraftInputs {
    /// The message being answered: the thread's newest member.
    original: MailContext,
    /// The thread's earlier messages, newest first.
    history: Vec<MailContext>,
    /// The account's style note, when one was built.
    style_note: Option<String>,
}

impl<P: Provider> App<P> {
    /// The reply-drafting driver: one future the host runs on its runtime
    /// for the life of the app, exactly the seam the incoming-pass and
    /// autosave drivers ride. Sleeps until an open arms a draft, drains
    /// the arming set one generation at a time, and loops: model latency
    /// never sits on the open's own path.
    pub async fn drive_reply_drafting(&self) {
        loop {
            self.reply_armed.notified().await;
            self.run_reply_drafting().await;
        }
    }

    /// Arms a reply draft for `message`: the open-message hook, and
    /// nothing more. The generation never runs here (the driver takes the
    /// arming); a message already armed waits below the in-set check, and
    /// one the pool already holds is dropped by the driver's own dedup.
    /// `correlation` is the reading turn's id, the open's mint; without
    /// the AI bundle the arm is a no-op and the driver sleeps on.
    pub(crate) fn request_reply_draft(
        &self,
        message: MessageRef,
        correlation: Option<CorrelationId>,
    ) {
        if self.ai.is_none() {
            return;
        }
        {
            let mut arms = self.reply_arms.lock().expect("reply-arms mutex poisoned");
            if arms.iter().any(|arm| arm.message == message) {
                return;
            }
            arms.push(DraftArm {
                message,
                correlation,
            });
        }
        self.reply_armed.notify_one();
    }

    /// Drains the arming set, one generation per message: the pool's
    /// presence (skipped rows included) is the once-per-message dedup, the
    /// arming's correlation rides the turn and a fresh one is minted for
    /// an arming that carried none, the context is collected through the
    /// engine's reads, and every landed row (drafted or skipped) signals
    /// the host's sink. Every failure logs and the drain moves on;
    /// nothing here may panic or propagate, the contract the open that
    /// armed it rests on.
    pub(crate) async fn run_reply_drafting(&self) {
        let Some(pass) = self.ai.as_ref() else {
            return;
        };
        let arms = std::mem::take(&mut *self.reply_arms.lock().expect("reply-arms mutex poisoned"));
        for arm in arms {
            if Self::already_drafted(&pass.store, &arm.message) {
                continue;
            }
            let correlation = arm.correlation.unwrap_or_else(CorrelationId::generate);
            let Some(inputs) = self.draft_inputs(&arm.message, &correlation, pass).await else {
                continue;
            };
            let context = DraftContext {
                original: &inputs.original,
                history: &inputs.history,
                style_note: inputs.style_note.as_deref(),
            };
            let landed =
                generate_reply(&context, &correlation, pass.complete.as_ref(), &pass.store).await;
            if let Some(entry) = landed {
                log::info!(
                    "ai replies: a pool row landed for account {} under correlation {correlation}",
                    entry.account.as_str(),
                );
                if let Some(sink) = pass.draft_sink.as_ref() {
                    sink.draft_pool_landed(MessageKey {
                        account: entry.account.clone(),
                        key: entry.key.clone(),
                    });
                }
            }
        }
    }

    /// Whether any earlier generation recorded a row for the message:
    /// the once-per-message dedup, skipped rows included (a failure a
    /// later open would just repeat is tried once). A read that fails
    /// answers drafted, the message waiting for a later open rather than
    /// spending a call the store cannot record beside.
    fn already_drafted(store: &AiStore, message: &MessageRef) -> bool {
        match store.draft_pool_for_message(message.account.as_str(), message.key.as_str()) {
            Ok(rows) => !rows.is_empty(),
            Err(err) => {
                log::warn!(
                    "ai replies: the pool read failed; the message waits for a later open: {err}"
                );
                true
            }
        }
    }

    /// Collects one armed message's draft inputs: the original resolved
    /// through the reading path with its body fetched, the thread's
    /// earlier members read through the engine's thread index with their
    /// bodies fetched, and the account's style note read (infering only
    /// when the daily cap says so). `None` when the original no longer
    /// resolves or its body cannot be fetched: the arming skips rather
    /// than drafting on half the thread, and the log names why.
    async fn draft_inputs(
        &self,
        message: &MessageRef,
        correlation: &CorrelationId,
        pass: &AiPass,
    ) -> Option<DraftInputs> {
        let handle = self.account_handle(&message.account).await?;
        let provider = handle.providers.first()?;
        let original = crate::reading::resolve(&self.engine, message).await?;
        let thread = original.thread_id().cloned();
        let body = match self
            .engine
            .message_body(provider, &message.account, &original)
            .await
        {
            Ok(body) => body.plain().map(str::to_owned).unwrap_or_default(),
            Err(err) => {
                log::warn!(
                    "ai replies: the original's body fetch failed; the draft is skipped: {err}"
                );
                return None;
            }
        };
        let original_context = MailContext {
            account: message.account.clone(),
            key: message.key.clone(),
            from_name: original
                .envelope
                .from
                .first()
                .map_or_else(String::new, |from| from.name.clone().unwrap_or_default()),
            from_address: original
                .envelope
                .from
                .first()
                .map_or_else(String::new, |from| from.email.clone()),
            subject: original.envelope.subject.clone().unwrap_or_default(),
            body,
        };
        let history = match thread.as_ref() {
            Some(thread) => {
                self.thread_history(&message.account, thread, &message.key)
                    .await
            }
            None => Vec::new(),
        };
        let style_note = self.style_note(&message.account, correlation, pass).await;
        Some(DraftInputs {
            original: original_context,
            history,
            style_note,
        })
    }

    /// The thread's earlier members as drafting contexts: the engine's
    /// thread read returns every member newest first, the original is
    /// dropped (it rides the prompt separately, last), the newest
    /// [`THREAD_HISTORY_LIMIT`](crate::THREAD_HISTORY_LIMIT) keep their
    /// bodies fetched, and a member whose body cannot be fetched is
    /// skipped whole rather than riding empty.
    async fn thread_history(
        &self,
        account: &AccountId,
        thread: &ThreadId,
        original: &ProviderKey,
    ) -> Vec<MailContext> {
        let rows = match self
            .engine
            .mail_on_threads(std::slice::from_ref(account), [thread.as_str()])
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                log::warn!("ai replies: the thread read failed; the history rides empty: {err}");
                Vec::new()
            }
        };
        let mut history = Vec::new();
        for row in rows
            .iter()
            .filter(|row| row.mail.key != *original)
            .take(THREAD_HISTORY_LIMIT)
        {
            let Some(body) = self.plain_body(account, &row.mail.key).await else {
                log::warn!("ai replies: a thread member's body fetch failed; it is skipped");
                continue;
            };
            history.push(member_context(row, body));
        }
        history
    }

    /// The account's style note for the turn: the Sent samples collected
    /// from the engine's reads, handed to the profile getter whose daily
    /// cap decides whether an infer call is spent. Demand-driven only; a
    /// fresh stored profile answers with no call.
    async fn style_note(
        &self,
        account: &AccountId,
        correlation: &CorrelationId,
        pass: &AiPass,
    ) -> Option<String> {
        let sent = self.sent_samples(account).await;
        style_profile(
            account,
            &sent,
            false,
            pass.complete.as_ref(),
            correlation,
            &pass.store,
        )
        .await
    }

    /// The account's recent Sent bodies, newest first, capped at
    /// [`STYLE_SAMPLE_LIMIT`]: the rows the newest-window read returns that
    /// the Sent mailbox holds, each with its body fetched. No Sent mailbox
    /// or an unreadable window answers empty, which keeps whatever
    /// profile the store holds.
    async fn sent_samples(&self, account: &AccountId) -> Vec<String> {
        let mailboxes = match self.engine.mailboxes(account).await {
            Ok(mailboxes) => mailboxes,
            Err(err) => {
                log::warn!("ai replies: the mailbox read failed; the style rides none: {err}");
                return Vec::new();
            }
        };
        let Some(sent) = mailboxes
            .iter()
            .find(|mailbox| mailbox.role == Some(MailboxRole::Sent))
            .map(|mailbox| mailbox.id.clone())
        else {
            return Vec::new();
        };
        let rows = match self
            .engine
            .mail_window(std::slice::from_ref(account), STYLE_SENT_WINDOW)
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                log::warn!("ai replies: the sent read failed; the style rides none: {err}");
                return Vec::new();
            }
        };
        let mut samples = Vec::new();
        for row in rows
            .iter()
            .filter(|row| row.mailboxes.contains(&sent))
            .take(STYLE_SAMPLE_LIMIT)
        {
            if let Some(body) = self.plain_body(account, &row.mail.key).await {
                samples.push(body);
            }
        }
        samples
    }

    /// One message's plain body, resolved fresh by key: the fetch the
    /// thread members and Sent samples share. `None` when the message no
    /// longer resolves, its account is gone, holds no provider, or the
    /// body cannot be fetched.
    async fn plain_body(&self, account: &AccountId, key: &ProviderKey) -> Option<String> {
        let handle = self.account_handle(account).await?;
        let provider = handle.providers.first()?;
        let message = MessageRef {
            account: account.clone(),
            key: key.clone(),
        };
        let original = crate::reading::resolve(&self.engine, &message).await?;
        let body = self
            .engine
            .message_body(provider, account, &original)
            .await
            .ok()?;
        Some(body.plain().map(str::to_owned).unwrap_or_default())
    }

    /// The message's smart-reply chips: the host pull the reading pane
    /// drives on open and on the draft-pool event, answered through the
    /// surfacing gate over the pool under a correlation minted for the
    /// pull's own turn. No bundle, no chips.
    pub async fn draft_chips(&self, account: &AccountId, key: &ProviderKey) -> Vec<Chip> {
        let Some(pass) = self.ai.as_ref() else {
            return Vec::new();
        };
        let correlation = CorrelationId::generate();
        chips_for(account, key, &pass.store, pass.judge.as_ref(), &correlation).await
    }

    /// Summarises the message: the cached answer when the store holds one
    /// for the body in front of the reader, else one completion through
    /// the bundle's seam under a correlation minted for the pull's own
    /// turn. `None`-shaped failures are named honestly: no bundle, or a
    /// message the store no longer resolves.
    ///
    /// # Errors
    /// [`SummariseMessageError::NoBundle`] when the app runs no AI
    /// bundle; [`SummariseMessageError::Unresolved`] when the message or
    /// its body cannot be collected; [`SummariseMessageError::Summarise`]
    /// when the action itself fails.
    pub async fn summarise_message(
        &self,
        account: &AccountId,
        key: &ProviderKey,
    ) -> Result<String, SummariseMessageError> {
        let Some(pass) = self.ai.as_ref() else {
            return Err(SummariseMessageError::NoBundle);
        };
        let handle = self
            .account_handle(account)
            .await
            .ok_or(SummariseMessageError::Unresolved)?;
        let provider = handle
            .providers
            .first()
            .ok_or(SummariseMessageError::Unresolved)?;
        let message = MessageRef {
            account: account.clone(),
            key: key.clone(),
        };
        let original = crate::reading::resolve(&self.engine, &message)
            .await
            .ok_or(SummariseMessageError::Unresolved)?;
        let body = self
            .engine
            .message_body(provider, account, &original)
            .await
            .map_err(|err| {
                log::warn!("ai replies: the summarise body fetch failed: {err}");
                SummariseMessageError::Unresolved
            })?;
        let mail = MailContext {
            account: account.clone(),
            key: key.clone(),
            from_name: original
                .envelope
                .from
                .first()
                .map_or_else(String::new, |from| from.name.clone().unwrap_or_default()),
            from_address: original
                .envelope
                .from
                .first()
                .map_or_else(String::new, |from| from.email.clone()),
            subject: original.envelope.subject.clone().unwrap_or_default(),
            body: body.plain().map(str::to_owned).unwrap_or_default(),
        };
        let correlation = CorrelationId::generate();
        Ok(summarise(&mail, &correlation, pass.complete.as_ref(), &pass.store).await?)
    }

    /// The account's style note, or `None` when none was built: the host
    /// pull behind the reading pane's style surface, reading through the
    /// same daily-capped getter the drafting turn does. No bundle, no
    /// note.
    pub async fn account_style_note(&self, account: &AccountId) -> Option<String> {
        let pass = self.ai.as_ref()?;
        let sent = self.sent_samples(account).await;
        let correlation = CorrelationId::generate();
        style_profile(
            account,
            &sent,
            false,
            pass.complete.as_ref(),
            &correlation,
            &pass.store,
        )
        .await
    }

    /// Records the reader's dismissal of the message's chips: the marker
    /// the surfacing gate consults before it spends a judge call,
    /// idempotent by the message identity pair. Without the AI bundle the
    /// write has nowhere to land and is a no-op.
    pub fn dismiss_chips(&self, account: &AccountId, key: &ProviderKey) {
        let Some(pass) = self.ai.as_ref() else {
            return;
        };
        if let Err(err) = pass.store.dismiss_chips(account.as_str(), key.as_str()) {
            log::warn!("ai replies: the chips dismissal could not be recorded: {err}");
        }
    }
}

/// One bodyless thread row as a drafting context: the row's own sender
/// and subject beside the fetched `body`.
fn member_context(row: &MailListRow, body: String) -> MailContext {
    MailContext {
        account: row.account.clone(),
        key: row.mail.key.clone(),
        from_name: row.mail.from_name.clone().unwrap_or_default(),
        from_address: row.mail.from_addr.clone().unwrap_or_default(),
        subject: row.mail.subject.clone().unwrap_or_default(),
        body,
    }
}

#[cfg(test)]
#[path = "drive_tests.rs"]
mod tests;
