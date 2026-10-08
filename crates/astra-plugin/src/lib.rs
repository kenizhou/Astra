// SPDX-License-Identifier: GPL-3.0-only
//! `astra-plugin`; the plugin host's contracts: the manifest a plugin declares,
//! the lifecycle it lives through, the versioned hook API it serves, and the
//! fail-closed permission table its grants pass.
//!
//! Plugins run out of process against a versioned hook API; hooks never touch the
//! engine, and send discipline in the host cannot be bypassed by anything here.
//! This crate is the SP1 skeleton (spec decision D6): types, parsing, the
//! lifecycle state machine, and the permission table only. There is no process
//! execution, no IPC, and no registry yet, so nothing in it runs a plugin.
//!
//! The security posture is fail closed: every permission defaults to
//! [`Approval::NeedsApproval`], a human decides every grant, and a manifest that
//! asks for a hook without declaring its permission never parses.

mod hook_api;
mod lifecycle;
mod manifest;
mod permissions;

pub use hook_api::{
    AccountRef, ComposeTransformRequest, ComposeTransformResponse, DecodedPart, HookApiV1,
    HookMetadata, MessageDecodeRequest, MessageDecodeResponse, SecurityBanner, SecurityLevel,
    HOOK_API_V1,
};
pub use lifecycle::{IllegalTransition, Lifecycle, LifecycleEvent};
pub use manifest::{ManifestError, PluginId, PluginManifest};
pub use permissions::{default_approval, Approval, Permission};
