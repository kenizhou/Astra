// SPDX-License-Identifier: GPL-3.0-only
//! The model tier vocabulary and the env-configured tier table.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::use_case::LlmUseCase;

/// One `provider:model` pair in a tier chain. The provider is lowercased on
/// parse and selects the wire protocol at call time (the `openai` and
/// `anthropic` prefixes of the dual-protocol client); the model keeps its
/// case. The spelling mirrors inbox-zero's tier-list syntax
/// (`apps/web/utils/llms/model.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEntry {
    /// The provider id, for example `openai` or `anthropic`.
    pub provider: String,
    /// The provider's model name.
    pub model: String,
}

/// The five model tiers. The wire words are lowercase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    /// The workhorse tier (inbox-zero `default`).
    Default,
    /// The bulk tier for cheap high-volume work.
    Economy,
    /// The conversational tier for user-visible chat.
    Chat,
    /// The drafting tier for composed prose.
    Draft,
    /// The smallest tier for classification-sized work.
    Nano,
}

impl ModelTier {
    /// The lowercase wire word.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Economy => "economy",
            Self::Chat => "chat",
            Self::Draft => "draft",
            Self::Nano => "nano",
        }
    }

    /// The environment variable holding this tier's list.
    pub fn env_var(&self) -> &'static str {
        match self {
            Self::Default => "ASTRA_LLM_DEFAULT",
            Self::Economy => "ASTRA_LLM_ECONOMY",
            Self::Chat => "ASTRA_LLM_CHAT",
            Self::Draft => "ASTRA_LLM_DRAFT",
            Self::Nano => "ASTRA_LLM_NANO",
        }
    }

    /// The tiers tried when this tier has no chain of its own: economy,
    /// chat, and draft fall back to default; nano falls back to economy
    /// then default (inbox-zero's fallback table).
    pub fn fallbacks(&self) -> &'static [ModelTier] {
        match self {
            Self::Economy | Self::Chat | Self::Draft => &[ModelTier::Default],
            Self::Nano => &[ModelTier::Economy, ModelTier::Default],
            Self::Default => &[],
        }
    }
}

impl std::fmt::Display for ModelTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The tiers in table order.
const ALL_TIERS: [ModelTier; 5] = [
    ModelTier::Default,
    ModelTier::Economy,
    ModelTier::Chat,
    ModelTier::Draft,
    ModelTier::Nano,
];

/// The configured tier lists: one ordered chain per tier that has one.
/// Built from the `ASTRA_LLM_*` environment variables.
#[derive(Debug)]
pub struct TierTable {
    chains: BTreeMap<ModelTier, Vec<ModelEntry>>,
}

impl TierTable {
    /// Reads the five `ASTRA_LLM_*` tier lists from the process
    /// environment. Parse failures are loud: a malformed entry refuses the
    /// whole table rather than skipping a model silently.
    ///
    /// # Errors
    /// Returns [`TierError::MalformedEntry`] for any entry that is not
    /// `provider:model` with both halves non-empty, and
    /// [`TierError::EmptyList`] for a set, non-blank value that carries no
    /// usable entry.
    pub fn from_env() -> Result<Self, TierError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// The parsing seam behind [`TierTable::from_env`]: `lookup` answers an
    /// environment-variable name with its value. An absent or blank answer
    /// leaves that tier unconfigured (resolution falls back at call time);
    /// a present value must parse.
    ///
    /// # Errors
    /// As [`TierTable::from_env`].
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, TierError> {
        let mut chains = BTreeMap::new();
        for tier in ALL_TIERS {
            let Some(raw) = lookup(tier.env_var()) else {
                continue;
            };
            if raw.trim().is_empty() {
                continue;
            }
            chains.insert(tier, parse_list(tier.env_var(), &raw)?);
        }
        Ok(Self { chains })
    }

    /// The ordered chain for a tier: its own list when configured, else the
    /// first configured tier among its fallbacks. The first entry is the
    /// primary model; the rest are fallbacks in order.
    ///
    /// # Errors
    /// Returns [`TierError::UnresolvedTier`] when neither the tier nor any
    /// of its fallbacks has a chain; the message names the environment
    /// variables to set.
    pub fn chain_for_tier(&self, tier: ModelTier) -> Result<&[ModelEntry], TierError> {
        for candidate in std::iter::once(tier).chain(tier.fallbacks().iter().copied()) {
            if let Some(chain) = self.chains.get(&candidate) {
                return Ok(chain);
            }
        }
        Err(TierError::UnresolvedTier {
            tier,
            vars: candidate_var_names(tier),
        })
    }

    /// Resolves a use case to its ordered `provider:model` chain: the
    /// use case names its tier, then [`TierTable::chain_for_tier`] applies
    /// the fallbacks.
    ///
    /// # Errors
    /// Returns [`TierError::UnresolvedUseCase`] when no chain stands behind
    /// the use case; the message names the environment variables to set.
    pub fn resolve(&self, use_case: LlmUseCase) -> Result<&[ModelEntry], TierError> {
        let tier = use_case.tier();
        match self.chain_for_tier(tier) {
            Ok(chain) => Ok(chain),
            Err(TierError::UnresolvedTier { vars, .. }) => Err(TierError::UnresolvedUseCase {
                use_case,
                tier,
                vars,
            }),
            Err(other) => Err(other),
        }
    }
}

/// Parses one comma-separated tier list (inbox-zero's syntax: split on
/// commas, trim, drop blank segments, split each entry on the first colon).
fn parse_list(var: &str, raw: &str) -> Result<Vec<ModelEntry>, TierError> {
    let mut entries = Vec::new();
    for segment in raw.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let Some((provider, model)) = segment.split_once(':') else {
            return Err(TierError::MalformedEntry {
                var: var.to_string(),
                entry: segment.to_string(),
            });
        };
        let provider = provider.trim().to_lowercase();
        let model = model.trim();
        if provider.is_empty() || model.is_empty() {
            return Err(TierError::MalformedEntry {
                var: var.to_string(),
                entry: segment.to_string(),
            });
        }
        entries.push(ModelEntry {
            provider,
            model: model.to_string(),
        });
    }
    if entries.is_empty() {
        return Err(TierError::EmptyList {
            var: var.to_string(),
        });
    }
    Ok(entries)
}

/// The `ASTRA_LLM_*` variable names a tier's resolution walked, for error
/// messages: the tier's own variable plus its fallbacks'.
fn candidate_var_names(tier: ModelTier) -> String {
    std::iter::once(tier)
        .chain(tier.fallbacks().iter().copied())
        .map(|tier| tier.env_var())
        .collect::<Vec<_>>()
        .join(" or ")
}

/// Failures of tier-list parsing and use-case resolution. Every variant is
/// loud by design: a misconfigured tier is never silently skipped.
#[derive(Debug, thiserror::Error)]
pub enum TierError {
    /// A tier-list entry is not `provider:model` with both halves
    /// non-empty.
    #[error("{var}: malformed tier entry {entry:?}; expected provider:model")]
    MalformedEntry {
        /// The `ASTRA_LLM_*` variable the entry came from.
        var: String,
        /// The offending entry.
        entry: String,
    },
    /// A set, non-blank tier list parsed to zero usable entries.
    #[error("{var}: no provider:model entries in a non-blank tier list")]
    EmptyList {
        /// The `ASTRA_LLM_*` variable the list came from.
        var: String,
    },
    /// No chain is configured for a tier or any of its fallbacks.
    #[error("no model chain for tier {tier}; set one of {vars}")]
    UnresolvedTier {
        /// The tier that had no chain.
        tier: ModelTier,
        /// The environment variables that would have provided one.
        vars: String,
    },
    /// [`TierTable::resolve`] found no chain behind a use case.
    #[error("no model chain for use case {use_case} (tier {tier}); set one of {vars}")]
    UnresolvedUseCase {
        /// The use case that could not resolve.
        use_case: LlmUseCase,
        /// The tier the use case names.
        tier: ModelTier,
        /// The environment variables that would have provided a chain.
        vars: String,
    },
}
