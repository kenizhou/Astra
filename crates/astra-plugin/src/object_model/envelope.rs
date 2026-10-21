// SPDX-License-Identifier: GPL-3.0-only
//! The envelope family: the whole of the worker protocol. One compact JSON
//! object plus a newline per message in each direction, `type`-tagged,
//! snake_case fields, stderr for diagnostics: the sidecar's discipline,
//! reused verbatim, as a plugin-specific family rather than a second meaning
//! inside the astra-app Intent enum.
//!
//! This family replaces the interim `protocol.rs` wholesale. `plugin_init`
//! carries the manifest, the granted permissions, the API level, the AOM
//! version, and the worker's private data directory, never the object tree;
//! every object interaction after that is an `object_call` answered by
//! `object_result` or `object_error`, an `object_subscribe` answered over
//! time by `object_event`s, or a `permission_request` the host's approval
//! path answers.

use serde::{Deserialize, Serialize};

use crate::{manifest::PluginManifest, permissions::Permission};

use super::refs::ObjectRef;

/// Host-to-plugin messages: the init handshake, object calls, event
/// subscriptions, and the orderly shutdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostMessage {
    /// The manifest this worker was started to serve, the permissions the
    /// host actually granted (a subset of the manifest's declared set), the
    /// pinned versions, and the worker's private data directory. The worker
    /// must never see another plugin's.
    PluginInit {
        /// The parsed manifest.
        manifest: PluginManifest,
        /// The permissions the host granted.
        grants: Vec<Permission>,
        /// The plugin API level the host speaks.
        api_level: u32,
        /// The AOM version the host speaks.
        aom_version: u32,
        /// The worker's private data directory.
        data_dir: String,
    },
    /// One object-model call, answered by `object_result` or `object_error`.
    /// The handle is absent for the calls that name no object (the session
    /// lookups that mint handles, for instance).
    ObjectCall {
        /// The invocation id the answer must echo.
        id: String,
        /// The object the call names, when it names one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        handle: Option<ObjectRef>,
        /// The method's dotted name, declared in the manifest.
        method: String,
        /// The method's arguments; the router validates but never interprets
        /// beyond the declared schema.
        args: serde_json::Value,
    },
    /// Subscribe to one event stream. Events arrive as `object_event` until
    /// the worker dies; death drains the subscription on the supervision path.
    ObjectSubscribe {
        /// The subscription id.
        id: String,
        /// The stream's dotted name, for example `explorer.onSelectionChange`.
        stream: String,
    },
    /// Ask the worker to exit. The host kills and reaps regardless of the
    /// answer, so an unresponsive worker cannot outlive its host.
    PluginShutdown,
}

/// Plugin-to-host messages: the ready handshake, call answers, subscription
/// events, permission asks, and worker-originated events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerMessage {
    /// The worker is ready to serve. The host checks the declared versions
    /// against its own before any call is forwarded.
    PluginReady {
        /// The plugin API level the worker speaks.
        api_level: u32,
        /// The AOM version the worker speaks.
        aom_version: u32,
        /// The methods the worker actually serves; a subset of the manifest's
        /// declared set.
        methods: Vec<String>,
    },
    /// A call succeeded.
    ObjectResult {
        /// The invocation id this answers.
        id: String,
        /// The call's result payload.
        value: serde_json::Value,
    },
    /// A call failed.
    ObjectError {
        /// The invocation id this answers.
        id: String,
        /// The failure, in the worker's own words.
        message: String,
    },
    /// One event on a subscribed stream.
    ObjectEvent {
        /// The stream's dotted name.
        stream: String,
        /// The event's payload.
        event: serde_json::Value,
    },
    /// The worker asks for a capability it was not granted. The host's
    /// approval path (a dialog, never a toast) answers.
    PermissionRequest {
        /// The request id the answer echoes.
        id: String,
        /// The capability asked for.
        capability: Permission,
        /// The worker's reason, shown to the human deciding.
        reason: String,
    },
    /// An out-of-band worker event (for example a key-store change), carrying
    /// no plaintext.
    PluginEvent {
        /// The event's own payload.
        event: serde_json::Value,
    },
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
