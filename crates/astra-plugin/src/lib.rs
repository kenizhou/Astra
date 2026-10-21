// SPDX-License-Identifier: GPL-3.0-only
//! `astra-plugin`; the plugin host's contracts: the manifest a plugin declares,
//! the lifecycle it lives through, the versioned object model (AOM) it speaks,
//! and the fail-closed permission table its grants pass.
//!
//! Plugins run out of process against the versioned object model; they see
//! host-issued handles and immutable snapshots, never the engine, and send
//! discipline in the host cannot be bypassed by anything here.
//!
//! The security posture is fail closed: every permission defaults to
//! [`Approval::NeedsApproval`], a human decides every grant, and a manifest
//! that asks for a method without declaring its permission never parses.

mod hook_api;
mod lifecycle;
mod manifest;
mod object_model;
mod permissions;

pub use hook_api::{
    AccountRef, ComposeTransformRequest, ComposeTransformResponse, DecodedPart, HookMetadata,
    MessageDecodeRequest, MessageDecodeResponse, SecurityBanner, SecurityLevel,
};
pub use lifecycle::{IllegalTransition, Lifecycle, LifecycleEvent};
pub use manifest::{ManifestError, PluginId, PluginManifest};
pub use object_model::{
    is_custom_method, lookup, AccountSnapshot, BlobRef, FolderSnapshot, HandleEntry, HandleKind,
    HandleTable, HostMessage, MessageSnapshot, MethodDef, NodeKind, ObjectRef, RefError, Stability,
    WorkerMessage, AOM_VERSION, METHODS, PLUGIN_API_LEVEL,
};
pub use permissions::{default_approval, Approval, Permission};
