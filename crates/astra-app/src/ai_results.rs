// SPDX-License-Identifier: GPL-3.0-only
//! The incoming AI pass: one bounded sweep over the newest newly seen
//! unread mail after each refresh, one correlation id per pass. The sweep
//! gates each candidate through the DLP policy before any stage prompt is
//! built, then runs the message's AI stages in order (the rule matches,
//! categorisation, the cold judge) and runs a matched rule's actions
//! through the deterministic write paths, everything recorded to the
//! results store (migration 0003) and the audit ledger. The seen tracking
//! is durable, not in-memory: a message with any results row is a message
//! a pass already judged, so a restart rejudges nothing.
//!
//! The core owns no executor (the runtime is the host's, the crate's own
//! rule), so a refresh only arms the pass: [`App::drive_incoming_pass`] is
//! the driver future the host runs for the life of the app, the same seam
//! the idle autosave driver rides. Failures log and the pass moves on;
//! nothing the pass does may fail the refresh that armed it.

use std::sync::Arc;

use astra_ai::{
    apply, AiStore, CorrelationId, DlpOutcome, DlpPolicy, JudgeOutcome, JudgePath, NewAiResult,
};
use engine_api::{AccountId, MailListRow, Provider, ProviderKey};

use crate::ai_rules::{
    categorise, judge_cold, match_rules, AiComplete, AiJudge, MailContext, Rule,
};
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

/// The AI stages a pass runs, as the results table's stage words: the
/// rule-level match and the message-level categorise and cold, beside the
/// gate's own row for a message the DLP policy refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AiStage {
    /// Does this rule's condition match the message? Abstains to no match.
    RuleMatch,
    /// Which of the five-way closed set is the message? Skips on failure.
    Categorise,
    /// Is the message unsolicited cold outreach? Abstains to not cold.
    Cold,
    /// The DLP gate's refusal row: the message's stages did not run.
    Dlp,
}

impl AiStage {
    /// The results table's wire word.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::RuleMatch => "rule_match",
            Self::Categorise => "categorise",
            Self::Cold => "cold",
            Self::Dlp => "dlp",
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

/// The AI bundle the app runs its pass over: the judge and completion
/// seams (the host's client behind them), the DLP policy every
/// mail-carrying prompt passes through, and the one store every AI path
/// shares (the rules, the results, the ledgers). Assembled host-side and
/// injected; the app without one simply never runs a pass.
pub struct AiPass {
    /// The judge seam: one binary judge call.
    pub(crate) judge: Box<dyn AiJudge>,
    /// The completion seam: one structured completion.
    pub(crate) complete: Box<dyn AiComplete>,
    /// The DLP policy the gate applies before any stage prompt.
    pub(crate) dlp: DlpPolicy,
    /// The shared AI store: rules in, results and audits out.
    pub(crate) store: Arc<AiStore>,
}

impl core::fmt::Debug for AiPass {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The seams may hold provider credentials; show nothing of them.
        f.debug_struct("AiPass").finish_non_exhaustive()
    }
}

impl AiPass {
    /// Bundles `judge`, `complete`, `store`, and the gate's `dlp` policy
    /// into the pass the app is given.
    #[must_use]
    pub fn new(
        judge: Box<dyn AiJudge>,
        complete: Box<dyn AiComplete>,
        store: Arc<AiStore>,
        dlp: DlpPolicy,
    ) -> Self {
        Self {
            judge,
            complete,
            dlp,
            store,
        }
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
    /// earlier pass judged, each gated through the DLP policy and then run
    /// through its stages (the rule match, categorise, the cold judge),
    /// and, on a match, executed through the deterministic write paths.
    /// Every failure logs and the pass moves on; nothing here may panic or
    /// propagate, the contract the refresh that armed it rests on.
    pub(crate) async fn run_incoming_pass(&self) {
        let Some(pass) = self.ai.as_ref() else {
            return;
        };
        let correlation = CorrelationId::generate();
        let rules = enabled_rules(&pass.store);
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
            let Some(raw) = self.match_context(row).await else {
                continue;
            };
            judged += 1;
            let Some(mail) = gate_mail(
                pass.dlp,
                raw,
                &row.account,
                &row.mail.key,
                &correlation,
                &pass.store,
            ) else {
                continue;
            };
            let matched = match_rules(
                &mail,
                &rules,
                &correlation,
                pass.judge.as_ref(),
                &pass.store,
            )
            .await;
            if let Err(err) =
                categorise(&mail, &correlation, pass.complete.as_ref(), &pass.store).await
            {
                log::warn!("ai pass: the categorise stage was skipped: {err}");
            }
            judge_cold(&mail, &correlation, pass.judge.as_ref(), &pass.store).await;
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

/// The DLP gate, in front of every stage prompt a message's text might
/// ride: allow passes the context through, redact masks every field the
/// stages build their prompts from (the sender, the subject, the body;
/// a sensitive span never crosses a field boundary), and block refuses the
/// message outright with one `dlp` row, the verdict `blocked`, and no seam
/// call spent. `None` means blocked: the caller skips the message's stages.
fn gate_mail(
    policy: DlpPolicy,
    mail: MailContext,
    account: &AccountId,
    key: &ProviderKey,
    correlation: &CorrelationId,
    store: &AiStore,
) -> Option<MailContext> {
    if policy == DlpPolicy::Allow {
        return Some(mail);
    }
    let mut mail = mail;
    for field in [
        &mut mail.from_name,
        &mut mail.from_address,
        &mut mail.subject,
        &mut mail.body,
    ] {
        if let DlpOutcome::Allowed { content } = apply(policy, field) {
            *field = content;
        } else {
            let row = NewAiResult {
                correlation: correlation.clone(),
                message_account: account.as_str().to_owned(),
                message_key: key.as_str().to_owned(),
                stage: AiStage::Dlp.as_str().to_owned(),
                rule_id: None,
                verdict: "blocked".to_owned(),
                category: None,
            };
            if let Err(err) = store.record_ai_result(&row) {
                log::warn!("ai pass: the dlp refusal could not be recorded: {err}");
            }
            log::info!("ai pass: the dlp policy blocked the message's ai stages");
            return None;
        }
    }
    Some(mail)
}

/// The enabled rules as the pass judges them: the stored rows parsed
/// loudly, a row outside the closed action set logged and skipped (the
/// pass must survive a broken row; the load-time rejection stays the
/// parse's own contract), the disabled ones dropped.
fn enabled_rules(store: &AiStore) -> Vec<Rule> {
    let rows = match store.list_rules() {
        Ok(rows) => rows,
        Err(err) => {
            log::warn!("ai pass: the rules read failed; the match stage is empty: {err}");
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
