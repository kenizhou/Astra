// SPDX-License-Identifier: GPL-3.0-only
//! The plugin manifest: the TOML declaration a plugin ships, parsed into
//! [`PluginManifest`] with every contract the host relies on enforced at parse.
//!
//! Parse is the first gate a plugin passes, so it rejects what must never reach a
//! running host: a blank or padded id, an unknown field, an API level other than
//! [`PLUGIN_API_LEVEL`](crate::PLUGIN_API_LEVEL), an AOM version other than
//! [`AOM_VERSION`](crate::AOM_VERSION), a method the registry does not define
//! (the custom `plugin.<id>.<name>` namespace excepted), a method or permission
//! declared twice, and a method declared without its required permission.
//! The manifest carries no secrets by design, which is what keeps its derived
//! `Debug` safe to log; any future field holding a secret must redact itself (as
//! `astra-account`'s `Secret` does) before it may live here.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    object_model::{is_custom_method, lookup, AOM_VERSION, PLUGIN_API_LEVEL},
    permissions::Permission,
};

/// How a `before_send` handler's verdict and its absence behave. Declared in
/// the manifest as `before_send_mode`; required when the handler is declared,
/// refused without it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendMode {
    /// Hard stop: cannot send until the handler approves, and an unavailable
    /// handler blocks (the default for crypto; no plaintext fallback).
    Block,
    /// Block on a violation, but fail open when the worker is unavailable.
    SoftBlock,
    /// The handler's reason becomes a Send Anyway / Deny decision the user
    /// answers; never remembered silently.
    PromptUser,
}

/// The kind of one settings field a plugin declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsFieldKind {
    /// An on/off switch.
    Toggle,
    /// A free-text input.
    Text,
    /// A choice among `options`.
    Select,
    /// A secret input, masked on screen.
    Password,
    /// A button the plugin answers.
    Action,
}

/// One field of a plugin's settings section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsFieldDescriptor {
    /// The field's key, the name the worker's answers name it by.
    pub key: String,
    /// Its kind.
    pub kind: SettingsFieldKind,
    /// Its label.
    pub label: String,
    /// The choices a `select` field offers; empty for every other kind.
    #[serde(default)]
    pub options: Vec<String>,
}

/// One settings section a plugin declares: a category in the settings screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsSectionDescriptor {
    /// The section's id.
    pub id: String,
    /// Its title.
    pub title: String,
    /// Its fields.
    #[serde(default)]
    pub fields: Vec<SettingsFieldDescriptor>,
}

/// One composer action a plugin declares: a button in the composer chrome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposerActionDescriptor {
    /// The action's id; its click calls the worker's `plugin.<id>.<id>`
    /// custom method.
    pub id: String,
    /// Its label.
    pub label: String,
    /// Its icon name, when it carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// One rich page a plugin declares: rendered inside the sandboxed frame over
/// the `astra-plugin://` scheme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageDescriptor {
    /// The page's id.
    pub id: String,
    /// Its title.
    pub title: String,
    /// Its entry HTML inside the plugin's bundle (for example `keys.html`).
    pub entry: String,
}

/// The UI descriptors a manifest may declare. Static shapes only: the content
/// a banner shows arrives at runtime (a decode answer or a plugin event),
/// never from this table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiDescriptors {
    /// The composer chrome buttons.
    #[serde(default)]
    pub composer_actions: Vec<ComposerActionDescriptor>,
    /// The settings categories.
    #[serde(default)]
    pub settings_sections: Vec<SettingsSectionDescriptor>,
    /// The rich pages for the sandboxed frame.
    #[serde(default)]
    pub pages: Vec<PageDescriptor>,
}

/// A plugin's identity, stable across versions: the id the host keys registry
/// entries and permission grants by. Declared once in the manifest and never
/// derived from anything. Parsing rejects ids that are blank or carry
/// surrounding whitespace (see [`ManifestError::BlankId`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// required field, an unknown field (including the retired `hook_api` and
    /// `hooks` fields), or a permission name the surface does not define.
    InvalidToml(toml::de::Error),
    /// The plugin id is empty, whitespace-only, or carries surrounding
    /// whitespace. The guard rejects rather than trims: the host keys registry
    /// entries and permission grants by the exact id string, so a silently
    /// trimmed id would let two manifests share one identity.
    BlankId,
    /// The manifest targets a plugin API level other than
    /// [`PLUGIN_API_LEVEL`](crate::PLUGIN_API_LEVEL). The levels in between do
    /// not exist for negotiation; the plugin is built for a different host than
    /// this one.
    ApiLevelMismatch {
        /// The level the manifest declared.
        found: u32,
        /// The level this host speaks.
        expected: u32,
    },
    /// The manifest targets an AOM version other than
    /// [`AOM_VERSION`](crate::AOM_VERSION).
    AomVersionMismatch {
        /// The version the manifest declared.
        found: u32,
        /// The version this host speaks.
        expected: u32,
    },
    /// The manifest declares no methods: an out-of-process plugin exists to
    /// serve methods, so a manifest naming none declares nothing to run.
    NoMethods,
    /// The manifest names a method the registry does not define and the custom
    /// `plugin.<id>.<name>` namespace does not cover.
    UnknownMethod(String),
    /// The manifest declares the same method more than once.
    DuplicateMethod(String),
    /// The manifest declares the same permission more than once.
    DuplicatePermission(Permission),
    /// The manifest declares a method without declaring the permission that
    /// method requires.
    MissingPermission {
        /// The method that needs the permission.
        method: String,
        /// The permission the manifest failed to declare.
        permission: Permission,
    },
    /// The manifest declares `before_send` but no `before_send_mode`.
    MissingSendMode,
    /// The manifest declares a `before_send_mode` without the `before_send`
    /// method it belongs to.
    UnexpectedSendMode,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::InvalidToml(err) => write!(f, "invalid manifest TOML: {err}"),
            ManifestError::BlankId => {
                write!(f, "plugin id is blank or carries surrounding whitespace")
            }
            ManifestError::ApiLevelMismatch { found, expected } => write!(
                f,
                "manifest targets plugin API level {found} but this host speaks {expected}"
            ),
            ManifestError::AomVersionMismatch { found, expected } => write!(
                f,
                "manifest targets AOM version {found} but this host speaks {expected}"
            ),
            ManifestError::NoMethods => write!(f, "manifest declares no methods"),
            ManifestError::UnknownMethod(method) => {
                write!(f, "manifest names unknown method {method}")
            }
            ManifestError::DuplicateMethod(method) => {
                write!(f, "manifest declares method {method} more than once")
            }
            ManifestError::DuplicatePermission(permission) => write!(
                f,
                "manifest declares permission {} more than once",
                permission.wire_name()
            ),
            ManifestError::MissingPermission { method, permission } => write!(
                f,
                "method {method} requires permission {}, which the manifest does not declare",
                permission.wire_name()
            ),
            ManifestError::MissingSendMode => {
                write!(f, "before_send is declared but before_send_mode is not")
            }
            ManifestError::UnexpectedSendMode => {
                write!(f, "before_send_mode is declared but before_send is not")
            }
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

/// A plugin's parsed manifest: what it is, which methods of which API level and
/// AOM version it serves, and the permissions it asks for. Parsing enforces the
/// whole declaration contract; a [`PluginManifest`] in hand is a manifest a
/// plugin author wrote correctly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    /// The plugin's identity, for example `dev.astra.crypto`.
    pub id: PluginId,
    /// The human-readable name shown in UI.
    pub name: String,
    /// The plugin's version, as semver.
    pub version: String,
    /// The plugin API level the plugin targets; must equal
    /// [`PLUGIN_API_LEVEL`](crate::PLUGIN_API_LEVEL).
    pub api_level: u32,
    /// The AOM version the plugin targets; must equal
    /// [`AOM_VERSION`](crate::AOM_VERSION).
    pub aom_version: u32,
    /// The methods the plugin serves or calls, as dotted names. Registry names
    /// are checked against the registry; `plugin.<id>.<name>` names are the
    /// open custom namespace.
    pub methods: Vec<String>,
    /// The permissions the plugin asks for. Parse verifies this set covers
    /// every declared registry method's required permission; whether the
    /// permissions are actually granted is the host's fail-closed decision,
    /// never the manifest's.
    pub permissions: Vec<Permission>,
    /// The send-gate mode, required exactly when the manifest declares the
    /// `before_send` method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_send_mode: Option<SendMode>,
    /// The UI descriptors the plugin declares, when it declares any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiDescriptors>,
    /// The publisher's ed25519 public key, lowercase hex. Signed bundles
    /// verify against exactly this key (see `bundle`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// The publisher's privacy policy, shown at install consent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_policy_url: Option<String>,
    /// The worker executable inside the plugin's directory, when the manifest
    /// sits beside its binary. Absent in tests and in manifests that name
    /// their binary by convention (the last id segment plus the OS suffix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
}

impl PluginManifest {
    /// Parses a manifest from its TOML text.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError::InvalidToml`] for malformed TOML, missing or
    /// unknown fields, and undefined permission names;
    /// [`ManifestError::BlankId`] when the id is empty, whitespace-only, or
    /// padded; [`ManifestError::ApiLevelMismatch`] or
    /// [`ManifestError::AomVersionMismatch`] when the pinned versions do not
    /// match this host; [`ManifestError::NoMethods`] when no method is
    /// declared; [`ManifestError::UnknownMethod`] for a method the registry
    /// does not define outside the custom namespace;
    /// [`ManifestError::DuplicateMethod`] or
    /// [`ManifestError::DuplicatePermission`] when either is declared twice;
    /// and [`ManifestError::MissingPermission`] when a declared method's
    /// required permission is not declared.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let manifest: PluginManifest = toml::from_str(text).map_err(ManifestError::InvalidToml)?;
        let id = manifest.id.as_str();
        if id.is_empty() || id.trim() != id {
            return Err(ManifestError::BlankId);
        }
        if manifest.api_level != PLUGIN_API_LEVEL {
            return Err(ManifestError::ApiLevelMismatch {
                found: manifest.api_level,
                expected: PLUGIN_API_LEVEL,
            });
        }
        if manifest.aom_version != AOM_VERSION {
            return Err(ManifestError::AomVersionMismatch {
                found: manifest.aom_version,
                expected: AOM_VERSION,
            });
        }
        if manifest.methods.is_empty() {
            return Err(ManifestError::NoMethods);
        }
        if let Some(method) = first_duplicate(&manifest.methods) {
            return Err(ManifestError::DuplicateMethod(method.clone()));
        }
        if let Some(permission) = first_duplicate(&manifest.permissions) {
            return Err(ManifestError::DuplicatePermission(*permission));
        }
        let serves_send_gate = manifest.methods.iter().any(|m| m == "before_send");
        match (serves_send_gate, manifest.before_send_mode) {
            (true, None) => return Err(ManifestError::MissingSendMode),
            (false, Some(_)) => return Err(ManifestError::UnexpectedSendMode),
            _ => {}
        }
        for method in &manifest.methods {
            if is_custom_method(method) {
                continue;
            }
            let Some(def) = lookup(method) else {
                return Err(ManifestError::UnknownMethod(method.clone()));
            };
            if !manifest.permissions.contains(&def.permission) {
                return Err(ManifestError::MissingPermission {
                    method: method.clone(),
                    permission: def.permission,
                });
            }
        }
        Ok(manifest)
    }
}

/// The first item appearing more than once in `items`, if any. Manifest arrays
/// are a handful of entries long, so the quadratic scan reads clearer than a
/// set.
fn first_duplicate<T: PartialEq>(items: &[T]) -> Option<&T> {
    items
        .iter()
        .enumerate()
        .find_map(|(index, item)| items[..index].contains(item).then_some(item))
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
