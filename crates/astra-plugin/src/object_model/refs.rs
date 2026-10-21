// SPDX-License-Identifier: GPL-3.0-only
//! Host-issued handles: the only way a plugin ever names an object.
//!
//! A handle is an opaque string the host minted, of the shape
//! `{kind}.{plugin}.{generation}.{serial}` (for example
//! `msg.dev.astra.echo.3.7`). The serial is a per-table counter: uniqueness
//! inside one table, never secrecy. The unforgeability is not in the string
//! but in the rule that the router resolves handles against its own table and
//! refuses anything it did not mint, so a guessed or client-composed string
//! (including a raw engine key like `imap:v1:u1`) is a typed refusal, not a
//! lookup.
//!
//! A handle binds the plugin id, the account, and the supervisor generation it
//! was minted under; it expires with the generation (a restarted worker holds
//! nothing), and one plugin's handles are foreign to another's table. The four
//! refusals are distinct so a plugin can tell a dead worker's handles
//! ([`RefError::StaleHandle`]) from a scope violation
//! ([`RefError::ForeignPlugin`], and the router's own `OutOfScope`).

use std::{collections::HashMap, error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::PluginId;

/// The object kinds a handle can name in this slice. The tree widens as later
/// slices route their nodes; each kind's wire name is fixed the day it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleKind {
    /// A stored message.
    Message,
    /// A folder in an account.
    Folder,
    /// A configured account.
    Account,
    /// A composition in progress.
    Draft,
}

impl HandleKind {
    /// The short prefix the kind carries inside a handle string.
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            HandleKind::Message => "msg",
            HandleKind::Folder => "fld",
            HandleKind::Account => "acc",
            HandleKind::Draft => "dft",
        }
    }

    /// Parses a kind prefix back from a handle string.
    #[must_use]
    pub fn from_wire_name(name: &str) -> Option<Self> {
        match name {
            "msg" => Some(HandleKind::Message),
            "fld" => Some(HandleKind::Folder),
            "acc" => Some(HandleKind::Account),
            "dft" => Some(HandleKind::Draft),
            _ => None,
        }
    }
}

/// An opaque object reference on the wire. The host mints it; the plugin holds
/// and returns it; neither side parses it except the table that minted it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectRef(String);

impl ObjectRef {
    /// Wraps a raw handle string. The router's table is the only caller with a
    /// reason to construct one from scratch; a plugin only ever returns
    /// handles it was handed.
    #[must_use]
    pub fn new(raw: String) -> Self {
        Self(raw)
    }

    /// The underlying handle string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a minted handle resolves to: the object's kind and the host-internal
/// identity the plugin never sees parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandleEntry {
    /// The object's kind.
    pub kind: HandleKind,
    /// The account the object belongs to.
    pub account: String,
    /// The host-internal key (for example the engine's `imap:v1:u1`), which
    /// never crosses the boundary itself.
    pub key: String,
}

/// Why a handle was refused. Every variant is a plugin-side programming error
/// or a liveness condition, never a retryable host failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefError {
    /// The table never minted this string: a guessed, forged, malformed, or
    /// client-composed reference.
    UnknownHandle(ObjectRef),
    /// The handle was minted under an earlier supervisor generation; the
    /// worker it belonged to has since restarted or died.
    StaleHandle {
        /// The refused handle.
        handle: ObjectRef,
        /// The generation the table serves now.
        current_generation: u64,
    },
    /// The handle belongs to another plugin's table.
    ForeignPlugin {
        /// The refused handle.
        handle: ObjectRef,
        /// The plugin this table serves.
        table_plugin: String,
    },
}

impl fmt::Display for RefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefError::UnknownHandle(handle) => {
                write!(f, "unknown handle {}", handle.as_str())
            }
            RefError::StaleHandle {
                handle,
                current_generation,
            } => write!(
                f,
                "handle {} was minted under an earlier generation; the table now serves {current_generation}",
                handle.as_str()
            ),
            RefError::ForeignPlugin {
                handle,
                table_plugin,
            } => write!(
                f,
                "handle {} does not belong to plugin {table_plugin}",
                handle.as_str()
            ),
        }
    }
}

impl Error for RefError {}

/// One plugin's handle table for one supervisor generation. The host keeps one
/// per (plugin, generation); a restart mints a fresh table and the old one's
/// handles answer [`RefError::StaleHandle`] ever after.
#[derive(Debug)]
pub struct HandleTable {
    plugin: PluginId,
    generation: u64,
    serial: u64,
    entries: HashMap<String, HandleEntry>,
}

impl HandleTable {
    /// An empty table for `plugin` under `generation`.
    #[must_use]
    pub fn new(plugin: PluginId, generation: u64) -> Self {
        Self {
            plugin,
            generation,
            serial: 0,
            entries: HashMap::new(),
        }
    }

    /// Mints a handle for one object, recording what it resolves to.
    pub fn mint(&mut self, kind: HandleKind, account: String, key: String) -> ObjectRef {
        self.serial += 1;
        let raw = format!(
            "{}.{}.{}.{}",
            kind.wire_name(),
            self.plugin.as_str(),
            self.generation,
            self.serial
        );
        self.entries
            .insert(raw.clone(), HandleEntry { kind, account, key });
        ObjectRef::new(raw)
    }

    /// Resolves a handle against this table. Anything this table did not mint
    /// is refused; the refusal names why.
    ///
    /// # Errors
    ///
    /// Returns [`RefError::UnknownHandle`] for a malformed, guessed, or
    /// never-minted string, [`RefError::StaleHandle`] for a handle minted
    /// under another generation, and [`RefError::ForeignPlugin`] for a handle
    /// carrying another plugin's id.
    pub fn resolve(&self, handle: &ObjectRef) -> Result<&HandleEntry, RefError> {
        let raw = handle.as_str();
        let unknown = || RefError::UnknownHandle(handle.clone());
        let segments: Vec<&str> = raw.split('.').collect();
        if segments.len() < 4 {
            return Err(unknown());
        }
        let (serial, generation) = (segments[segments.len() - 1], segments[segments.len() - 2]);
        if serial.parse::<u64>().is_err() || HandleKind::from_wire_name(segments[0]).is_none() {
            return Err(unknown());
        }
        let Ok(generation) = generation.parse::<u64>() else {
            return Err(unknown());
        };
        if generation != self.generation {
            return Err(RefError::StaleHandle {
                handle: handle.clone(),
                current_generation: self.generation,
            });
        }
        let plugin = segments[1..segments.len() - 2].join(".");
        if plugin != self.plugin.as_str() {
            return Err(RefError::ForeignPlugin {
                handle: handle.clone(),
                table_plugin: self.plugin.as_str().to_string(),
            });
        }
        self.entries.get(raw).ok_or_else(unknown)
    }
}

#[cfg(test)]
#[path = "refs_tests.rs"]
mod tests;
