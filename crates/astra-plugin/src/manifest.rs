// SPDX-License-Identifier: GPL-3.0-only
//! The plugin manifest: the TOML declaration a plugin ships, parsed into
//! [`PluginManifest`] with every contract the host relies on enforced at parse.
//!
//! Parse is the first gate a plugin passes, so it rejects what must never reach a
//! running host: an unknown field, a hook API level other than
//! [`HOOK_API_V1`](crate::HOOK_API_V1), a hook name or permission name the surface
//! does not define, and a hook served without its required permission declared.
//! The manifest carries no secrets by design, which is what keeps its derived
//! `Debug` safe to log; any future field holding a secret must redact itself (as
//! `astra-account`'s `Secret` does) before it may live here.

use std::{error::Error, fmt};

use serde::Deserialize;

use crate::{
    hook_api::{HookApiV1, HOOK_API_V1},
    permissions::Permission,
};

/// A plugin's identity, stable across versions: the id the host keys registry
/// entries and permission grants by. Declared once in the manifest and never
/// derived from anything.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct PluginId(String);

impl PluginId {
    /// Wraps a raw id string.
    #[must_use]
    pub fn new(raw: String) -> Self {
        Self(raw)
    }

    /// The raw id string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a manifest was rejected. Every variant is a plugin author's error to fix
/// in the manifest, not a host condition to retry.
#[derive(Debug)]
pub enum ManifestError {
    /// The TOML is not a manifest this crate can read: malformed TOML, a missing
    /// required field, an unknown field, or a hook or permission name the surface
    /// does not define.
    InvalidToml(toml::de::Error),
    /// The manifest targets a hook API level other than [`HOOK_API_V1`]. The
    /// levels in between do not exist for negotiation; the plugin is built for a
    /// different host than this one.
    ApiLevelMismatch {
        /// The level the manifest declared.
        found: u32,
        /// The level this host speaks.
        expected: u32,
    },
    /// The manifest serves a hook without declaring the permission that hook
    /// requires.
    MissingPermission {
        /// The hook that needs the permission.
        hook: HookApiV1,
        /// The permission the manifest failed to declare.
        permission: Permission,
    },
    /// The manifest declares no hooks: an out-of-process plugin exists to serve
    /// hooks, so a manifest naming none declares nothing to run.
    NoHooks,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::InvalidToml(err) => write!(f, "invalid manifest TOML: {err}"),
            ManifestError::ApiLevelMismatch { found, expected } => write!(
                f,
                "manifest targets hook API level {found} but this host speaks {expected}"
            ),
            ManifestError::MissingPermission { hook, permission } => write!(
                f,
                "hook {} requires permission {}, which the manifest does not declare",
                hook.wire_name(),
                permission.wire_name()
            ),
            ManifestError::NoHooks => write!(f, "manifest declares no hooks"),
        }
    }
}

impl Error for ManifestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ManifestError::InvalidToml(err) => Some(err),
            _ => None,
        }
    }
}

/// A plugin's parsed manifest: what it is, which hooks of which API level it
/// serves, and the permissions it asks for. Parsing enforces the whole
/// declaration contract; a [`PluginManifest`] in hand is a manifest a plugin
/// author wrote correctly.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    /// The plugin's identity, for example `dev.astra.crypto`.
    pub id: PluginId,
    /// The human-readable name shown in UI.
    pub name: String,
    /// The plugin's version, as semver.
    pub version: String,
    /// The hook API level the plugin targets; must equal
    /// [`HOOK_API_V1`](crate::HOOK_API_V1).
    pub hook_api: u32,
    /// The hooks the plugin serves.
    pub hooks: Vec<HookApiV1>,
    /// The permissions the plugin asks for. Parse verifies this set covers every
    /// declared hook's required permission; whether the permissions are actually
    /// granted is the host's fail-closed decision, never the manifest's.
    pub permissions: Vec<Permission>,
}

impl PluginManifest {
    /// Parses a manifest from its TOML text.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError::InvalidToml`] for malformed TOML, missing or
    /// unknown fields, and undefined hook or permission names;
    /// [`ManifestError::ApiLevelMismatch`] when `hook_api` is not
    /// [`HOOK_API_V1`](crate::HOOK_API_V1); [`ManifestError::NoHooks`] when no
    /// hook is declared; and [`ManifestError::MissingPermission`] when a declared
    /// hook's required permission is not declared.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let manifest: PluginManifest = toml::from_str(text).map_err(ManifestError::InvalidToml)?;
        if manifest.hook_api != HOOK_API_V1 {
            return Err(ManifestError::ApiLevelMismatch {
                found: manifest.hook_api,
                expected: HOOK_API_V1,
            });
        }
        if manifest.hooks.is_empty() {
            return Err(ManifestError::NoHooks);
        }
        for hook in HookApiV1::ALL {
            if !manifest.hooks.contains(&hook) {
                continue;
            }
            let permission = hook.required_permission();
            if !manifest.permissions.contains(&permission) {
                return Err(ManifestError::MissingPermission { hook, permission });
            }
        }
        Ok(manifest)
    }
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
