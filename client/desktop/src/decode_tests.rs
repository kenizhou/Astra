// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the composite decoder: a crypto verdict takes precedence and
//! ends the fold (the plugin is never consulted for a recognised message,
//! even when the verdict is fail closed), a non-encrypted message falls
//! through to the plugin, and two silent decoders answer nothing.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use astra_app::{DecodeOutcome, MessageDecoder, MessageRef};
use astra_viewmodel::{SecurityBannerView, SecurityLevelView};

use super::CompositeDecoder;

/// A scripted decoder: answers the canned outcome, counting its calls.
struct ScriptedDecoder {
    outcome: Option<DecodeOutcome>,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl MessageDecoder for ScriptedDecoder {
    async fn decode(&self, _message: &MessageRef, _raw: &[u8]) -> Option<DecodeOutcome> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.outcome.clone()
    }
}

fn message() -> MessageRef {
    MessageRef::from_parts("work", "m1".to_owned()).expect("a message ref")
}

/// A fail-closed crypto verdict: the banner alone, no legs.
fn closed_verdict() -> DecodeOutcome {
    DecodeOutcome {
        banner: Some(SecurityBannerView {
            level: SecurityLevelView::Error,
            text: "Encrypted message; cannot decrypt (no key).".to_owned(),
        }),
        crypto_verdict: true,
        ..Default::default()
    }
}

/// A decrypted crypto verdict: the legs and the verified banner.
fn decrypted_verdict() -> DecodeOutcome {
    DecodeOutcome {
        html: Some("<p>the words</p>".to_owned()),
        plain: Some("the words".to_owned()),
        banner: Some(SecurityBannerView {
            level: SecurityLevelView::Info,
            text: "Encrypted message; signature verified.".to_owned(),
        }),
        subject: Some("the real subject".to_owned()),
        crypto_verdict: true,
        password_prompt: None,
    }
}

/// The plugin's own answer, the fall-through's payload.
fn plugin_answer() -> DecodeOutcome {
    DecodeOutcome {
        plain: Some("plugin words".to_owned()),
        ..Default::default()
    }
}

fn scripted(outcome: Option<DecodeOutcome>) -> (Arc<ScriptedDecoder>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let decoder = Arc::new(ScriptedDecoder {
        outcome,
        calls: Arc::clone(&calls),
    });
    (decoder, calls)
}

#[test]
fn a_closed_crypto_verdict_takes_precedence_and_never_falls_through() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let (crypto, crypto_calls) = scripted(Some(closed_verdict()));
        let (plugin, plugin_calls) = scripted(Some(plugin_answer()));
        let composite = CompositeDecoder::new(crypto, plugin);

        let outcome = composite
            .decode(&message(), b"raw")
            .await
            .expect("the verdict stands");

        assert_eq!(outcome, closed_verdict());
        assert_eq!(crypto_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            plugin_calls.load(Ordering::SeqCst),
            0,
            "a recognised message never reaches the plugin"
        );
    });
}

#[test]
fn a_decrypted_crypto_verdict_takes_precedence() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let (crypto, _) = scripted(Some(decrypted_verdict()));
        let (plugin, plugin_calls) = scripted(Some(plugin_answer()));
        let composite = CompositeDecoder::new(crypto, plugin);

        let outcome = composite
            .decode(&message(), b"raw")
            .await
            .expect("the verdict stands");

        assert_eq!(outcome, decrypted_verdict());
        assert_eq!(plugin_calls.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn a_non_encrypted_message_falls_through_to_the_plugin() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let (crypto, crypto_calls) = scripted(None);
        let (plugin, plugin_calls) = scripted(Some(plugin_answer()));
        let composite = CompositeDecoder::new(crypto, plugin);

        let outcome = composite
            .decode(&message(), b"raw")
            .await
            .expect("the plugin answers");

        assert_eq!(outcome, plugin_answer());
        assert_eq!(crypto_calls.load(Ordering::SeqCst), 1);
        assert_eq!(plugin_calls.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn two_silent_decoders_answer_nothing() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let (crypto, _) = scripted(None);
        let (plugin, _) = scripted(None);
        let composite = CompositeDecoder::new(crypto, plugin);
        assert_eq!(composite.decode(&message(), b"raw").await, None);
    });
}
