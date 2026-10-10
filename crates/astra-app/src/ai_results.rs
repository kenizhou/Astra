// SPDX-License-Identifier: GPL-3.0-only
//! The incoming AI pass: one bounded sweep over the newest newly seen
//! unread mail after each refresh, one correlation id per pass. The sweep
//! judges each candidate against the enabled rules through the match
//! stage and runs a matched rule's actions through the deterministic
//! write paths, everything recorded to the results store (migration
//! 0003) and the audit ledger. The seen tracking is durable, not
//! in-memory: a message with any results row is a message a pass already
//! judged, so a restart rejudges nothing.
//!
//! The core owns no executor (the runtime is the host's, the crate's own
//! rule), so a refresh only arms the pass: [`App::drive_incoming_pass`] is
//! the driver future the host runs for the life of the app, the same seam
//! the idle autosave driver rides. Failures log and the pass moves on;
//! nothing the pass does may fail the refresh that armed it.

use std::sync::Arc;

use astra_ai::{AiStore, CorrelationId, JudgeOutcome, JudgePath};
use engine_api::{AccountId, MailListRow, Provider, ProviderKey};

use crate::ai_rules::{match_rules, AiJudge, MailContext, Rule};
use crate::reference::MessageRef;
use crate::App;

/// How many messages one pass judges: the bound that keeps a cold mailbox
/// from spending a model call on every historical row. The head N of the
/// newest; what falls below waits for a later pass.
pub(crate) const PASS_MESSAGE_LIMIT: usize = 10;

/// The newest-window the pass draws candidates from: the store read is
/// cheap, and unread-but-seen and already-read rows filter out of it
/// before any model call is spent.
pub(crate) const PASS_CANDIDATE_WINDOW: usize = 50;

/// The AI stages a pass runs, as the results table's stage words. The A1
/// set is the rule match; the cold and categorise stages arrive with their
/// slice and grow this closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AiStage {
    /// Does this rule's condition match the message? Abstains to no match.
    RuleMatch,
}

impl AiStage {
    /// The results table's wire word.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::RuleMatch => "rule_match",
        }
    }
}

/// The outcome word one judge result records: the verdict the caller acted
/// on, except that a safe default's answer names itself, so a row's `no`
/// is a model's no and its `abstain_default` is the table's.
#[must_use]
pub(crate) fn outcome_word(outcome: JudgeOutcome) -> &'static str {
    match outcome.path {
        JudgePath::FirstTry | JudgePath::Retry => outcome.verdict.as_str(),
        JudgePath::AbstainDefault => "abstain_default",
    }
}

/// The AI bundle the app runs its pass over: the judge seam (the host's
/// client behind it) and the one store every AI path shares (the rules,
/// the results, the ledgers). Assembled host-side and injected; the app
/// without one simply never runs a pass.
pub struct AiPass {
    /// The judge seam: one binary judge call.
    pub(crate) judge: Box<dyn AiJudge>,
    /// The shared AI store: rules in, results and audits out.
    pub(crate) store: Arc<AiStore>,
}

impl core::fmt::Debug for AiPass {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The judge seam may hold provider credentials; show nothing of it.
        f.debug_struct("AiPass").finish_non_exhaustive()
    }
}

impl AiPass {
    /// Bundles `judge` and `store` into the pass the app is given.
    #[must_use]
    pub fn new(judge: Box<dyn AiJudge>, store: Arc<AiStore>) -> Self {
        Self { judge, store }
    }
}

impl<P: Provider> App<P> {
    /// The incoming-pass driver: one future the host runs on its runtime
    /// for the life of the app, exactly the seam the idle autosave driver
    /// rides. Sleeps until a refresh arms the pass, runs one pass at a
    /// time, and loops: model latency never sits on the refresh path that
    /// armed it.
    pub async fn drive_incoming_pass(&self) {
        loop {
            self.pass_armed.notified().await;
            self.run_incoming_pass().await;
        }
    }

    /// Arms the pass for the driver: the refresh hook, and nothing more.
    /// A pass runs only when the AI bundle is in place; with none the arm
    /// is a no-op and the driver sleeps on.
    pub(crate) fn request_incoming_pass(&self) {
        if self.ai.is_some() {
            self.pass_armed.notify_one();
        }
    }

    /// One bounded pass: one fresh correlation id, the enabled rules, and
    /// the head [`PASS_MESSAGE_LIMIT`] of the newest unread messages no
    /// earlier pass judged, each matched and, on a match, executed through
    /// the deterministic write paths. Every failure logs and the pass
    /// moves on; nothing here may panic or propagate, the contract the
    /// refresh that armed it rests on.
    pub(crate) async fn run_incoming_pass(&self) {
        let Some(pass) = self.ai.as_ref() else {
            return;
        };
        let correlation = CorrelationId::generate();
        let rules = enabled_rules(&pass.store);
        if rules.is_empty() {
            log::info!("ai pass: no enabled rules; nothing to judge");
            return;
        }
        let accounts = self.account_handles().await;
        let ids: Vec<AccountId> = accounts.iter().map(|account| account.id.clone()).collect();
        let rows = match self.engine.mail_window(&ids, PASS_CANDIDATE_WINDOW).await {
            Ok(rows) => rows,
            Err(err) => {
                log::warn!("ai pass: the candidate read failed; the pass is skipped: {err}");
                return;
            }
        };
        let mut judged = 0usize;
        for row in &rows {
            if judged >= PASS_MESSAGE_LIMIT {
                break;
            }
            if !row.mail.flags.is_unread() {
                continue;
            }
            if Self::already_judged(&pass.store, &row.account, &row.mail.key) {
                continue;
            }
            let Some(context) = self.match_context(row).await else {
                continue;
            };
            let matched = match_rules(
                &context,
                &rules,
                &correlation,
                pass.judge.as_ref(),
                &pass.store,
            )
            .await;
            judged += 1;
            let message = MessageRef {
                account: row.account.clone(),
                key: row.mail.key.clone(),
            };
            for result in &matched {
                log::info!(
                    "ai pass: rule {} matched; running its actions",
                    result.rule.name,
                );
                self.execute_actions(&message, &result.rule.actions, &correlation, &pass.store)
                    .await;
            }
        }
        log::info!("ai pass: judged {judged} message(s) under correlation {correlation}");
    }

    /// Whether any earlier pass recorded a result for the message: the
    /// durable seen check. A read that fails skips the message rather than
    /// spends a call the store cannot record beside.
    fn already_judged(store: &AiStore, account: &AccountId, key: &ProviderKey) -> bool {
        match store.ai_results_for_message(account.as_str(), key.as_str()) {
            Ok(rows) => !rows.is_empty(),
            Err(err) => {
                log::warn!(
                    "ai pass: the seen read failed; the message waits for a later pass: {err}"
                );
                true
            }
        }
    }

    /// Builds one candidate's match context: the row's sender and subject
    /// beside the body read through the engine's cache-first path. `None`
    /// when the message no longer resolves, its account is gone, or the
    /// body cannot be fetched: the candidate waits for a later pass rather
    /// than being judged on half its text, and the log names why.
    async fn match_context(&self, row: &MailListRow) -> Option<MailContext> {
        let message = MessageRef {
            account: row.account.clone(),
            key: row.mail.key.clone(),
        };
        let Some(handle) = self.account_handle(&row.account).await else {
            log::warn!("ai pass: the candidate's account is no longer configured");
            return None;
        };
        let Some(provider) = handle.providers.first() else {
            log::warn!("ai pass: the candidate's account has no provider to read through");
            return None;
        };
        let Some(original) = crate::reading::resolve(&self.engine, &message).await else {
            log::warn!("ai pass: the candidate no longer resolves in the store");
            return None;
        };
        let plain = match self
            .engine
            .message_body(provider, &row.account, &original)
            .await
        {
            Ok(body) => body.plain().map(str::to_owned).unwrap_or_default(),
            Err(err) => {
                log::warn!("ai pass: the candidate's body fetch failed: {err}");
                return None;
            }
        };
        Some(MailContext {
            account: row.account.clone(),
            key: row.mail.key.clone(),
            from_name: row.mail.from_name.clone().unwrap_or_default(),
            from_address: row.mail.from_addr.clone().unwrap_or_default(),
            subject: row.mail.subject.clone().unwrap_or_default(),
            body: plain,
        })
    }
}

/// The enabled rules as the pass judges them: the stored rows parsed
/// loudly, a row outside the closed action set logged and skipped (the
/// pass must survive a broken row; the load-time rejection stays the
/// parse's own contract), the disabled ones dropped.
fn enabled_rules(store: &AiStore) -> Vec<Rule> {
    let rows = match store.list_rules() {
        Ok(rows) => rows,
        Err(err) => {
            log::warn!("ai pass: the rules read failed; the pass is skipped: {err}");
            return Vec::new();
        }
    };
    rows.into_iter()
        .filter_map(|row| match Rule::try_from(row) {
            Ok(rule) => rule.enabled.then_some(rule),
            Err(err) => {
                log::error!("ai pass: a stored rule was skipped: {err}");
                None
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "ai_results_tests.rs"]
mod tests;
