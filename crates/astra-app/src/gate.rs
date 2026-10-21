// SPDX-License-Identifier: GPL-3.0-only
//! The send gate and compose transform ports: how the app asks the host's
//! plugins before a send may queue.
//!
//! The gate runs where the app still owns the decision: in every submit arm,
//! after the draft builds and before `submit_mail` queues it (the engine owns
//! the queue and its scheduling, so a veto after queueing is not a veto at
//! all). A composition the gate blocks is never queued: it stands as a held
//! send in the sending snapshot, and the draft the autosave already persisted
//! is what a retry re-gates.
//!
//! Both ports are host-installed and optional: with no plugin serving
//! `before_send` or `composer.transform`, no port is installed and a submit
//! costs nothing. The desktop host's `PluginGate` is the implementation; the
//! tests drive scripted ones.
//!
//! **Privacy.** A composition is the user's. Nothing here logs its content,
//! its recipients, or its subject; the held row carries the plugin's id and
//! its reason, never the body.

use async_trait::async_trait;
use engine_api::{AccountId, EmailAddress};

/// One composition at the send gate: the built draft's envelope and both
/// rendered bodies, everything a handler vets or rewrites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateComposition {
    /// The account the send leaves from.
    pub account: AccountId,
    /// The sender's address.
    pub from: EmailAddress,
    /// The To recipients.
    pub to: Vec<EmailAddress>,
    /// The Cc recipients.
    pub cc: Vec<EmailAddress>,
    /// The Bcc recipients.
    pub bcc: Vec<EmailAddress>,
    /// The subject line.
    pub subject: String,
    /// The rendered plain-text leg.
    pub plain: String,
    /// The rendered HTML leg.
    pub html: String,
}

/// The gate's answer for one composition. The host folds every serving
/// handler's answer into one verdict before the app sees it: block beats
/// prompt beats allow, and the first block wins the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateVerdict {
    /// Every handler approved; the send may queue.
    Allow,
    /// A prompt-user handler asked a question: the person answers Send Anyway
    /// or Deny. Carries the plugin's id and its reason, both shown.
    PromptUser {
        /// The plugin asking.
        plugin: String,
        /// Its reason, in its own words, shown verbatim.
        reason: String,
    },
    /// A block-mode handler refused, timed out, or was unavailable: the send
    /// holds. Carries the plugin's id and the reason the held row shows.
    Block {
        /// The plugin holding the send.
        plugin: String,
        /// Why, in product words.
        reason: String,
    },
}

/// A transform failure: the send holds with the transform named, because
/// sending the untransformed mail would be the plaintext fallback the crypto
/// posture refuses on purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformFailure {
    /// The plugin whose transform failed or was unavailable.
    pub plugin: String,
    /// Why, in product words.
    pub reason: String,
}

/// The send gate: the host's plugin verdicts, one call per submit.
#[async_trait]
pub trait BeforeSendGate: Send + Sync {
    /// Answers the folded verdict for one built composition.
    async fn check(&self, composition: &GateComposition) -> GateVerdict;
}

/// The compose transform: the host's plugins rewrite the rendered bodies in
/// manifest order, each seeing the previous one's output.
#[async_trait]
pub trait ComposeTransform: Send + Sync {
    /// Transforms one composition's bodies, or fails closed.
    async fn transform(
        &self,
        composition: GateComposition,
    ) -> Result<GateComposition, TransformFailure>;
}
