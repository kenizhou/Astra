// SPDX-License-Identifier: GPL-3.0-only
//! Hook API v1: the two out-of-process hooks and their typed request/response
//! contracts.
//!
//! These are versioned wire types, shaped exactly as they will cross the plugin
//! boundary: snake_case field names, externally tagged enums, and absent
//! `Option`s omitted rather than nulled. SP1 ships them as compiled, unit-tested
//! contracts only; no hook is executed, because no composer or process host
//! exists yet. Changing any wire shape under [`HOOK_API_V1`] is a breaking
//! contract change; the versioned process such changes will follow lives in the
//! plugin contract document once it is written.
//!
//! Hooks never touch the engine: a hook sees only the payload carried in its
//! request, and send discipline in the host cannot be bypassed by a response.

use serde::{Deserialize, Serialize};

use crate::permissions::Permission;

/// The hook API level this crate speaks. A manifest must declare exactly this
/// level; anything else is rejected at parse (see [`crate::PluginManifest`]).
pub const HOOK_API_V1: u32 = 1;

/// The hooks of API level 1. A manifest names the ones a plugin serves by their
/// dotted wire names (for example `compose.transform`), and
/// [`HookApiV1::required_permission`] is what each of them costs in declared
/// permissions. The serde names are the same dotted strings, so a manifest, the
/// wire, and error messages all cite one name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookApiV1 {
    /// Pre-send transform (encrypt, sign) at the composer's output seam.
    #[serde(rename = "compose.transform")]
    ComposeTransform,
    /// Post-fetch decode (decrypt, verify) producing renderable parts and
    /// security banner data.
    #[serde(rename = "message.decode")]
    MessageDecode,
}

impl HookApiV1 {
    /// Every hook of API level 1.
    pub const ALL: [HookApiV1; 2] = [HookApiV1::ComposeTransform, HookApiV1::MessageDecode];

    /// The hook's dotted wire name: the string a manifest declares, the serde
    /// envelope carries, and error messages cite.
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            HookApiV1::ComposeTransform => "compose.transform",
            HookApiV1::MessageDecode => "message.decode",
        }
    }

    /// The permission a plugin must declare to serve this hook. The manifest
    /// parse enforces that every declared hook's permission is declared too.
    #[must_use]
    pub fn required_permission(self) -> Permission {
        match self {
            HookApiV1::ComposeTransform => Permission::TransformOutgoing,
            HookApiV1::MessageDecode => Permission::DecodeIncoming,
        }
    }
}

/// The host's opaque reference to the account a hook invocation belongs to. A
/// plain string on the wire; the host resolves it, the plugin never can.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountRef(String);

impl AccountRef {
    /// Wraps the host's account reference.
    #[must_use]
    pub fn new(raw: String) -> Self {
        Self(raw)
    }

    /// The underlying reference string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Context about the message a hook is invoked for. Both fields are absent at
/// some invocation sites, so both are optional and omitted when `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HookMetadata {
    /// The account the message belongs to, when the host can name one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountRef>,
    /// The message's RFC 5322 `Message-ID`, when one is known. A message being
    /// composed has none yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

/// The `compose.transform` request: the composer's output the plugin transforms
/// before send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComposeTransformRequest {
    /// The composer's HTML body.
    pub composer_html: String,
    /// The composer's plain-text body, when one exists alongside the HTML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composer_plain: Option<String>,
    /// Context about the message being composed.
    pub metadata: HookMetadata,
}

/// The `compose.transform` response: the transformed bodies the host sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComposeTransformResponse {
    /// The transformed HTML body.
    pub html: String,
    /// The transformed plain-text body, when the plugin produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain: Option<String>,
}

/// One renderable part a `message.decode` produced: a MIME type and its content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DecodedPart {
    /// The part's MIME type, for example `text/plain`.
    pub mime_type: String,
    /// The part's decoded content.
    pub content: String,
}

/// How much the reader should trust what the banner says about a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityLevel {
    /// Neutral context: the banner states a fact, good or neutral.
    Info,
    /// The message is usable but something about it did not verify.
    Warning,
    /// The message or its crypto is broken; the reader should distrust it.
    Error,
}

/// The security banner data a `message.decode` returns for the reading pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecurityBanner {
    /// How much to trust the statement the banner makes.
    pub level: SecurityLevel,
    /// The human-readable statement, for example "Signature invalid".
    pub text: String,
}

/// The `message.decode` request: one fetched raw message to decode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageDecodeRequest {
    /// The raw RFC 5322 message, undecoded.
    pub raw_message: String,
    /// Context about the message being decoded.
    pub metadata: HookMetadata,
}

/// The `message.decode` response: renderable parts plus optional security
/// banner data for the reading pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageDecodeResponse {
    /// The decoded parts to render, in order.
    pub decoded_parts: Vec<DecodedPart>,
    /// The security banner to show above them, when the decode has something to
    /// state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_banner: Option<SecurityBanner>,
}

#[cfg(test)]
#[path = "hook_api_tests.rs"]
mod tests;
