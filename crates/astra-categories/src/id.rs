// SPDX-License-Identifier: GPL-3.0-only
//! The category identity: the slug a definition is keyed by, carried across
//! the host boundary and into the row wire as a newtype, never a raw string.

use serde::{Deserialize, Serialize};

/// The id of one category definition: the name's slug (suffixed on collision),
/// the key the assignment rows reference and the row wire carries. Serialises
/// as its bare string so it crosses process boundaries without a wrapper
/// object.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CategoryId(String);

impl CategoryId {
    /// Wraps an existing id string.
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CategoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
