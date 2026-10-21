// SPDX-License-Identifier: GPL-3.0-only
//! The registry's persistence: `grants.toml` beside the managed plugins
//! directory, the truth across restarts. The in-memory table is the
//! write-through cache: every approve, enable, and disable lands on disk
//! before it answers, and a fresh registry reads the file at construction. A
//! malformed file reads as empty rather than failing the boot.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use astra_plugin::Permission;
use serde::{Deserialize, Serialize};

/// One plugin's persisted record: its granted set and whether it is enabled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PluginRecord {
    /// The permissions the host granted.
    #[serde(default)]
    pub(crate) grants: Vec<Permission>,
    /// Whether the plugin is enabled (its worker starts at boot).
    #[serde(default)]
    pub(crate) enabled: bool,
}

/// The on-disk shape: one record per plugin id.
#[derive(Debug, Default, Serialize, Deserialize)]
struct GrantsFile {
    #[serde(default)]
    plugins: BTreeMap<String, PluginRecord>,
}

/// The file's path under one registry root.
fn file_of(root: &Path) -> PathBuf {
    root.join("grants.toml")
}

/// The records a registry root holds; a missing or malformed file is empty.
pub(crate) fn load(root: &Path) -> BTreeMap<String, PluginRecord> {
    let Ok(text) = std::fs::read_to_string(file_of(root)) else {
        return BTreeMap::new();
    };
    match toml::from_str::<GrantsFile>(&text) {
        Ok(file) => file.plugins,
        Err(error) => {
            log::warn!("plugins: the grants file does not parse; starting empty: {error}");
            BTreeMap::new()
        }
    }
}

/// Writes the records. A failed write is a warning, never a broken boot: the
/// in-memory table still answers this session.
pub(crate) fn store(root: &Path, records: &BTreeMap<String, PluginRecord>) {
    let file = GrantsFile {
        plugins: records.clone(),
    };
    match toml::to_string(&file) {
        Ok(text) => {
            if let Err(error) = std::fs::write(file_of(root), text) {
                log::warn!("plugins: the grants file could not be written: {error}");
            }
        }
        Err(error) => log::warn!("plugins: the grants file could not be encoded: {error}"),
    }
}
