//! The MIME-wrap seam for outbound mail: a host-installed transform over an
//! assembled message, consulted per draft before the transport dials. The
//! seam exists so message crypto (S/MIME, OpenPGP) wraps *inside* the
//! engine's own submit path — one send engine, one outbox, one drain —
//! instead of a second submitter beside it.

use async_trait::async_trait;

use crate::{Draft, ProviderError};

/// The two byte-strings one wrap produces: what goes on the wire (no `Bcc`
/// header, so no recipient can see the blind list) and what is filed as the
/// sender's Sent copy (`Bcc` included, so the sender keeps the record). One
/// wrap per send — the two differ only in that header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MimeWrapOutcome {
    /// The over-the-wire message.
    pub wire: Vec<u8>,
    /// The filed Sent copy.
    pub filed: Vec<u8>,
}

/// A host-installed transform over an assembled outbound message.
///
/// The engine consults it only when one is installed AND the draft asks
/// ([`Draft::sign`] or [`Draft::encrypt`]); every other send is byte-identical
/// to a build without the seam. It receives the **filed** assembly (with the
/// `Bcc` header) and returns both variants — the caller wraps once per send,
/// and the wire variant is the same ciphertext minus the blind list.
///
/// Everything here runs entirely before the transport dials and before
/// `HandOver::commit`: a refusal therefore carries no delivery ambiguity, and
/// the honest refusal is [`ProviderError::permanent`] (a gate the user must
/// resolve — say, a recipient without an accepted key), which settles the op
/// `Failed` with the reason and lets the outbox's Send Now re-attempt the
/// wrap. Never `needs_confirmation`: nothing has reached the server.
#[async_trait]
pub trait MimeWrapper: Send + Sync {
    /// Wraps `filed_message` (the filed assembly of `draft`) and returns both
    /// variants, or refuses with a [`ProviderError`] (see the trait docs for
    /// the classification contract).
    async fn wrap(
        &self,
        draft: &Draft,
        filed_message: Vec<u8>,
    ) -> Result<MimeWrapOutcome, ProviderError>;
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use engine_core::{ids::MessageIdHeader, mail::EmailAddress};

    use super::*;

    /// A scripted wrapper: records what it saw and answers a canned outcome or
    /// a canned refusal.
    struct Script {
        seen: Mutex<Vec<(bool, bool, Vec<u8>)>>,
        refuse: bool,
    }

    #[async_trait]
    impl MimeWrapper for Script {
        async fn wrap(
            &self,
            draft: &Draft,
            filed_message: Vec<u8>,
        ) -> Result<MimeWrapOutcome, ProviderError> {
            self.seen.lock().expect("script lock").push((
                draft.sign,
                draft.encrypt,
                filed_message.clone(),
            ));
            if self.refuse {
                return Err(ProviderError::permanent("a scripted refusal"));
            }
            Ok(MimeWrapOutcome {
                wire: b"wire-variant".to_vec(),
                filed: filed_message,
            })
        }
    }

    fn draft() -> Draft {
        Draft::new(
            MessageIdHeader::new("m1@example.test").expect("a message id"),
            EmailAddress::new("sender@example.test"),
            vec![EmailAddress::new("rcpt@example.test")],
            "subject",
            "body",
        )
    }

    #[tokio::test]
    async fn a_wrapper_sees_the_flags_and_the_filed_assembly() {
        let script = Script {
            seen: Mutex::new(Vec::new()),
            refuse: false,
        };
        let draft = draft().with_crypto(true, false);
        let outcome = script
            .wrap(&draft, b"filed-assembly".to_vec())
            .await
            .expect("wrap succeeds");
        assert_eq!(outcome.wire, b"wire-variant".to_vec());
        assert_eq!(outcome.filed, b"filed-assembly".to_vec());
        let seen = script.seen.lock().expect("script lock");
        assert_eq!(seen.len(), 1);
        assert!(seen[0].0, "the sign flag crosses");
        assert!(!seen[0].1, "the encrypt flag crosses");
        assert_eq!(seen[0].2, b"filed-assembly".to_vec());
    }

    #[tokio::test]
    async fn a_refusal_is_a_permanent_error() {
        let script = Script {
            seen: Mutex::new(Vec::new()),
            refuse: true,
        };
        let err = script
            .wrap(&draft().with_crypto(true, true), b"filed".to_vec())
            .await
            .expect_err("the refusal crosses");
        assert!(
            !err.class().is_retryable(),
            "a gate refusal must never retry blindly: {err}"
        );
        assert!(err.detail().contains("a scripted refusal"));
    }
}
