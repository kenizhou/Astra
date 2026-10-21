// SPDX-License-Identifier: GPL-3.0-only
//! The plugin decoder: the desktop host's implementation of astra-app's
//! `MessageDecoder` port over the registry's activated workers.
//!
//! The first activated plugin serving `message.decode` under the `read_item`
//! grant answers; its parts become the reading's legs (text/html part to
//! html, text/plain to plain) and its banner rides the snapshot. A missing,
//! erroring, or slow worker answers nothing and the store's own rendering
//! stands: decode fails open because a plugin being down must not blank the
//! user's mail.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content; the
//! raw source crosses only to the serving worker, under its grant.

use std::{sync::Arc, time::Duration};

use astra_app::{DecodeOutcome, MessageDecoder, MessageRef};
use astra_plugin::{
    AccountRef, HookMetadata, MessageDecodeRequest, MessageDecodeResponse, Permission,
};
use astra_plugin::{HandleKind, SecurityLevel};

use super::PluginRegistry;

/// The decoder over one registry. Installed at boot; the registry is read at
/// call time, so a plugin enabled later joins without a reinstall.
pub(crate) struct PluginDecoder {
    registry: Arc<PluginRegistry>,
    /// The hard cap on one decode call (the gate's cap; the tests name
    /// milliseconds).
    timeout: Duration,
}

impl PluginDecoder {
    /// A decoder over `registry` with `timeout` per call.
    #[must_use]
    pub(crate) fn new(registry: Arc<PluginRegistry>, timeout: Duration) -> Self {
        Self { registry, timeout }
    }
}

/// The banner's level as the viewmodel spells it.
fn banner_level(level: SecurityLevel) -> astra_viewmodel::SecurityLevelView {
    match level {
        SecurityLevel::Info => astra_viewmodel::SecurityLevelView::Info,
        SecurityLevel::Warning => astra_viewmodel::SecurityLevelView::Warning,
        SecurityLevel::Error => astra_viewmodel::SecurityLevelView::Error,
    }
}

#[async_trait::async_trait]
impl MessageDecoder for PluginDecoder {
    async fn decode(&self, message: &MessageRef, raw: &[u8]) -> Option<DecodeOutcome> {
        let worker = self
            .registry
            .serving("message.decode", Permission::ReadItem)
            .into_iter()
            .next()?;
        let handle = worker
            .host
            .mint_handle(
                HandleKind::Message,
                message.account.as_str(),
                message.key.as_str(),
            )
            .await;
        let request = MessageDecodeRequest {
            raw_message: String::from_utf8_lossy(raw).into_owned(),
            metadata: HookMetadata {
                account: Some(AccountRef::new(message.account.as_str().to_owned())),
                message_id: None,
            },
        };
        let payload = serde_json::to_value(&request).expect("the request serialises");
        let call = worker.host.call(Some(handle), "message.decode", payload);
        let value = tokio::time::timeout(self.timeout, call)
            .await
            .ok()
            .and_then(Result::ok)?;
        let response: MessageDecodeResponse = serde_json::from_value(value).ok()?;
        let mut outcome = DecodeOutcome {
            html: None,
            plain: None,
            banner: response
                .security_banner
                .map(|banner| astra_viewmodel::SecurityBannerView {
                    level: banner_level(banner.level),
                    text: banner.text,
                }),
        };
        for part in response.decoded_parts {
            match part.mime_type.as_str() {
                "text/html" => outcome.html = Some(part.content),
                "text/plain" => outcome.plain = Some(part.content),
                _ => {}
            }
        }
        Some(outcome)
    }
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
