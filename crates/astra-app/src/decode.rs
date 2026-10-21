// SPDX-License-Identifier: GPL-3.0-only
//! The message decode port: how the app asks the host's plugins to decode one
//! fetched message in the reading pipeline.
//!
//! The seam sits in `render_reading`, between the resolve and the render: the
//! raw source is read (`engine.message_source`, cache-first) and handed to
//! the installed decoder, and the answer's parts replace the rendered legs:
//! the decoded HTML through the identical sanitise / inline-cid / remote-gate
//! pipeline every other body takes, the decoded plain as the text view, with
//! the answer's banner riding the snapshot's `security_banner` leg. No
//! decoder, an error, or a timeout: today's rendering, no banner. The decode
//! sees raw bytes only under the plugin's `read_item` grant, and the host
//! mints the call's message handle; a plugin that answers nothing changes
//! nothing.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content; the
//! port's answer carries the body's legs, which only ever flow to the reading
//! snapshot.

use astra_viewmodel::SecurityBannerView;

/// One decode's answer: the replacement legs and the banner, each optional.
/// Both legs absent and no banner is the decoder saying nothing, which the
/// pipeline treats as no answer at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeOutcome {
    /// The decoded HTML leg, sanitised by the same pipeline as any body
    /// before it renders.
    pub html: Option<String>,
    /// The decoded plain-text leg.
    pub plain: Option<String>,
    /// The security banner for the reading pane.
    pub banner: Option<SecurityBannerView>,
}

/// The message decoder: the host's plugin decode, one call per open.
#[async_trait::async_trait]
pub trait MessageDecoder: Send + Sync {
    /// Decodes one message's raw source. `None` leaves the store's own
    /// rendering in place: decode fails open because a plugin being down must
    /// not blank the user's mail.
    async fn decode(
        &self,
        message: &crate::reference::MessageRef,
        raw: &[u8],
    ) -> Option<DecodeOutcome>;
}

impl<P: engine_api::Provider> crate::App<P> {
    /// Names the plugin decoder every reading open consults, or `None` (the
    /// default) to render the store's own read. Call it before the first
    /// dispatch; the host's decoder reads its plugin registry at call time,
    /// so plugins enabled later join without a reinstall.
    #[must_use]
    pub fn with_decoder(mut self, decoder: Option<std::sync::Arc<dyn MessageDecoder>>) -> Self {
        self.decoder = decoder;
        self
    }
}
