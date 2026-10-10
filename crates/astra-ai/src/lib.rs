// SPDX-License-Identifier: GPL-3.0-only
//! `astra-ai`: the AI foundation's registry and ledgers.
//!
//! The crate owns what every AI path shares: the use-case registry
//! ([`LlmUseCase`], the SPA0 six plus the judge family), the model tier
//! vocabulary ([`ModelTier`]) and the env-configured [`TierTable`] that
//! resolves a use case to an ordered provider:model chain (first entry
//! primary, the rest fallback), the [`CorrelationId`] that ties one agent
//! turn together across the host, the sidecar, the CLI, and both ledgers,
//! the product SQLite store ([`AiStore`], the product database's
//! arrival), the prompt-injection hardening types ([`tag_untrusted`] and
//! [`DlpPolicy`], whose redact and block arms are resolved to allow until
//! the A1 hardening slice), and the binary judge's vocabulary
//! ([`JudgeVerdict`], [`parse_judgement`], the [`SAFE_DEFAULTS`] table,
//! and the versioned [`judge_prompt`] template).
//!
//! Nothing here calls a model: the background client lives in the desktop
//! host and consumes this crate.

mod correlation;
mod hardening;
mod judge;
mod store;
mod tier;
mod use_case;

#[cfg(test)]
mod hardening_tests;
#[cfg(test)]
mod judge_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod tier_tests;

pub use correlation::CorrelationId;
pub use hardening::{tag_untrusted, DlpPolicy};
pub use judge::{
    judge_prompt, parse_judgement, safe_default, JudgeDefault, JudgeOutcome, JudgePath,
    JudgeUseCase, JudgeVerdict, ABSTAIN_MARKER, JUDGE_TEMPLATES_VERSION, SAFE_DEFAULTS,
};
pub use store::{AiStore, AiStoreError, AuditRow, LlmCallRow};
pub use tier::{ModelEntry, ModelTier, TierError, TierTable};
pub use use_case::LlmUseCase;
