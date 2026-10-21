// SPDX-License-Identifier: GPL-3.0-only
//! The plugin gate: the desktop host's implementation of astra-app's
//! `BeforeSendGate` and `ComposeTransform` ports over the registry's
//! activated workers.
//!
//! `check` folds every serving plugin's `before_send` answer into the one
//! verdict the app sees (block beats prompt beats allow; the first block wins
//! the reason), applying each plugin's declared mode to an unavailable or
//! slow worker: `block` fails closed, `soft_block` and `prompt_user` fail
//! open (a prompt plugin that never answers never had a question to show).
//! Auto-activation costs the admin grant (the centralized-deployment rule): a
//! plugin granted send but not admin is never called.
//!
//! `transform` chains every serving plugin's `composer.transform` answer in
//! registry order, each seeing the previous one's output, and fails closed on
//! an error or a timeout: sending the untransformed mail would be the
//! plaintext fallback the crypto posture refuses on purpose.
//!
//! **Privacy.** A composition is the user's. Nothing here logs its content,
//! its recipients, or its subject; plugin ids and reasons only.

use std::{sync::Arc, time::Duration};

use astra_app::{BeforeSendGate, ComposeTransform, GateComposition, GateVerdict, TransformFailure};
use astra_plugin::{Permission, SendMode};

use super::PluginRegistry;

/// The verdict a worker's answer names: allow, or a violation with its
/// reason. Anything else the worker said is not a verdict and is an error.
enum Answer {
    Allow,
    Violation(String),
}

/// Parses a worker's `before_send` answer: `{"verdict": "allow"}` or
/// `{"verdict": "block"|"prompt", "reason": "..."}`. The mode, not the
/// answer's verb, decides what the app sees of a violation.
fn verdict_of(value: &serde_json::Value) -> Option<Answer> {
    match value["verdict"].as_str() {
        Some("allow") => Some(Answer::Allow),
        Some("block" | "prompt") => Some(Answer::Violation(
            value["reason"]
                .as_str()
                .unwrap_or("the plugin refused this send")
                .to_string(),
        )),
        _ => None,
    }
}

/// The gate over one registry. Installed at boot; the registry is read at
/// call time, so a plugin enabled later joins without a reinstall, and one
/// disabled drops out at the next send.
pub(crate) struct PluginGate {
    registry: Arc<PluginRegistry>,
    /// The hard cap on one handler call (the plan's ~5min in production; the
    /// tests name milliseconds).
    timeout: Duration,
}

impl PluginGate {
    /// A gate over `registry` with `timeout` per handler call.
    #[must_use]
    pub(crate) fn new(registry: Arc<PluginRegistry>, timeout: Duration) -> Self {
        Self { registry, timeout }
    }

    /// The composition as the call's payload: envelope fields and both
    /// rendered legs, addresses as bare strings.
    fn payload_of(composition: &GateComposition) -> serde_json::Value {
        let addresses = |list: &[engine_api::EmailAddress]| {
            list.iter()
                .map(|address| address.email.clone())
                .collect::<Vec<_>>()
        };
        serde_json::json!({
            "account": composition.account.as_str(),
            "from": composition.from.email,
            "to": addresses(&composition.to),
            "cc": addresses(&composition.cc),
            "bcc": addresses(&composition.bcc),
            "subject": composition.subject,
            "plain": composition.plain,
            "html": composition.html,
        })
    }

    /// One plugin's call under the timeout; `None` for a timeout or an
    /// error answer, which the caller's mode folds.
    async fn ask(
        &self,
        worker: &super::ServingWorker,
        method: &str,
        payload: serde_json::Value,
    ) -> Option<serde_json::Value> {
        match tokio::time::timeout(self.timeout, worker.host.call(None, method, payload)).await {
            Ok(Ok(value)) => Some(value),
            Ok(Err(error)) => {
                log::info!(
                    "plugins: {} answered {method} with an error: {error}",
                    worker.id
                );
                None
            }
            Err(_) => {
                log::info!(
                    "plugins: {} did not answer {method} inside {:?}",
                    worker.id,
                    self.timeout
                );
                None
            }
        }
    }
}

#[async_trait::async_trait]
impl BeforeSendGate for PluginGate {
    async fn check(&self, composition: &GateComposition) -> GateVerdict {
        let mut prompt: Option<(String, String)> = None;
        for worker in self
            .registry
            .serving("before_send", Permission::Send)
            .into_iter()
            // Auto-activation costs the admin grant: a plugin granted send
            // but not admin is never called.
            .filter(|worker| self.registry.has_grant(&worker.id, Permission::Admin))
        {
            let mode = worker.mode.unwrap_or(SendMode::Block);
            let answer = self
                .ask(&worker, "before_send", Self::payload_of(composition))
                .await;
            match (mode, answer) {
                (SendMode::Block, None) => {
                    return GateVerdict::Block {
                        plugin: worker.id.clone(),
                        reason: "the plugin is unavailable".to_owned(),
                    };
                }
                (SendMode::SoftBlock | SendMode::PromptUser, None) => {}
                (_, Some(value)) => match verdict_of(&value) {
                    Some(Answer::Allow) => {}
                    Some(Answer::Violation(reason)) => match mode {
                        SendMode::Block | SendMode::SoftBlock => {
                            return GateVerdict::Block {
                                plugin: worker.id.clone(),
                                reason,
                            };
                        }
                        SendMode::PromptUser => {
                            prompt.get_or_insert((worker.id.clone(), reason));
                        }
                    },
                    None => {
                        // A malformed answer is an error; the mode folds it.
                        if mode == SendMode::Block {
                            return GateVerdict::Block {
                                plugin: worker.id.clone(),
                                reason: "the plugin's answer was not a verdict".to_owned(),
                            };
                        }
                    }
                },
            }
        }
        match prompt {
            Some((plugin, reason)) => GateVerdict::PromptUser { plugin, reason },
            None => GateVerdict::Allow,
        }
    }
}

#[async_trait::async_trait]
impl ComposeTransform for PluginGate {
    async fn transform(
        &self,
        mut composition: GateComposition,
    ) -> Result<GateComposition, TransformFailure> {
        for worker in self
            .registry
            .serving("composer.transform", Permission::ComposeEdit)
        {
            let payload = serde_json::json!({
                "composer_html": composition.html,
                "composer_plain": composition.plain,
                "metadata": { "account": composition.account.as_str() },
            });
            let Some(value) = self.ask(&worker, "composer.transform", payload).await else {
                return Err(TransformFailure {
                    plugin: worker.id.clone(),
                    reason: "the transform is unavailable".to_owned(),
                });
            };
            let Some(html) = value["html"].as_str() else {
                return Err(TransformFailure {
                    plugin: worker.id.clone(),
                    reason: "the transform's answer was not a document".to_owned(),
                });
            };
            html.clone_into(&mut composition.html);
            if let Some(plain) = value["plain"].as_str() {
                plain.clone_into(&mut composition.plain);
            }
        }
        Ok(composition)
    }
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
