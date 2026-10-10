// SPDX-License-Identifier: GPL-3.0-only
//! `astra-ai`: the AI foundation's registry, ledgers, and shared helpers.
//!
//! The crate owns what every AI path shares: the use-case registry
//! ([`LlmUseCase`], the SPA0 six plus the judge family and the A1 rules
//! generation use case), the model tier vocabulary ([`ModelTier`]) and
//! the env-configured [`TierTable`] that resolves a use case to an ordered
//! provider:model chain (first entry primary, the rest fallback), the
//! [`CorrelationId`] that ties one agent turn together across the host,
//! the sidecar, the CLI, and both ledgers, the product SQLite store
//! ([`AiStore`], the product database's arrival, with the rules CRUD of
//! migration 0002 beside the ledgers, the draft pool and style profiles
//! of 0004, the summarise cache of 0005, and the chips-dismissed markers
//! of 0006), the prompt-injection
//! hardening wrapper ([`tag_untrusted`]), the active data loss
//! prevention gate ([`DlpPolicy`] with its [`redact`] and [`apply`]),
//! the structured output [`repair_json`] helper every structured caller
//! recovers through, and the binary judge's vocabulary ([`JudgeVerdict`],
//! [`parse_judgement`], the [`SAFE_DEFAULTS`] table, and the versioned
//! [`judge_prompt`] template).
//!
//! Nothing here calls a model: the background client lives in the desktop
//! host and consumes this crate.

mod ai_results;
mod chip_dismissals;
mod correlation;
mod dlp;
mod draft_pool;
mod hardening;
mod judge;
mod repair;
mod rules;
mod store;
mod summarise_cache;
mod tier;
mod use_case;

#[cfg(test)]
mod ai_results_tests;
#[cfg(test)]
mod chip_dismissals_tests;
#[cfg(test)]
mod dlp_tests;
#[cfg(test)]
mod draft_pool_tests;
#[cfg(test)]
mod hardening_tests;
#[cfg(test)]
mod judge_tests;
#[cfg(test)]
mod repair_tests;
#[cfg(test)]
mod rules_store_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod summarise_cache_tests;
#[cfg(test)]
mod tier_tests;

pub use ai_results::{AiResultRow, NewAiResult};
pub use correlation::CorrelationId;
pub use dlp::{apply, redact, DlpError, DlpOutcome, DlpPolicy};
pub use draft_pool::{DraftPoolRow, NewDraftPoolRow, StyleProfileRow};
pub use hardening::tag_untrusted;
pub use judge::{
    judge_prompt, parse_judgement, safe_default, JudgeDefault, JudgeOutcome, JudgePath,
    JudgeUseCase, JudgeVerdict, ABSTAIN_MARKER, JUDGE_TEMPLATES_VERSION, SAFE_DEFAULTS,
};
pub use repair::repair_json;
pub use rules::{NewRule, RuleId, RuleRow};
pub use store::{AiStore, AiStoreError, AuditRow, LlmCallRow};
pub use summarise_cache::SummariseCacheRow;
pub use tier::{ModelEntry, ModelTier, TierError, TierTable};
pub use use_case::LlmUseCase;
