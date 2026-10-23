//! Submission preparation: the [`Submission`] a draft becomes (wire message, filed
//! variant, envelope, EHLO identity) and the assemble-then-wrap helper behind it,
//! shared by the send path ([`crate::filing`]'s `submit`) and the Sent-copy repair
//! ([`crate::filing::refile`]), so both wrap — or plain-assemble — through one code
//! path. Split out of `filing` for the file-length budget.
//!
//! The contract (`docs/agent-guidance/imap-smtp.md`): when a host-installed
//! [`MimeWrapper`] is present **and** the draft asks ([`Draft::sign`] or
//! [`Draft::encrypt`]), the FILED variant is assembled and wrapped once, and
//! the wire variant is the same ciphertext minus the `Bcc` header. Every
//! other send assembles the wire variant exactly as before, byte for byte.

use std::{collections::HashSet, sync::Arc};

use engine_provider::{Draft, MimeWrapOutcome, MimeWrapper, ProviderResult};
use engine_rfc5322::{assemble_filed_message, assemble_message};
use time::OffsetDateTime;

/// Everything derived from a draft once, shared by the wire send and the filed copy —
/// so a STARTTLS send (which negotiates before transmitting) and a plaintext/implicit
/// one run identical preparation and filing around the differing transmit step.
pub(crate) struct Submission {
    /// One timestamp for both the transmitted and filed copy (they differ only in Bcc).
    pub(crate) now: OffsetDateTime,
    /// The over-the-wire message — **without** the `Bcc` header.
    pub(crate) message: Vec<u8>,
    /// The filed Sent copy, already wrapped by the account's MIME wrapper (`Some`
    /// only when a wrapper ran; then `message` is its wire variant). `None` for a
    /// plain send, whose filed copy is derived by the old rule (reuse when no Bcc,
    /// else a filed re-assembly).
    pub(crate) filed: Option<Vec<u8>>,
    /// Envelope `MAIL FROM` address.
    pub(crate) from: String,
    /// De-duplicated envelope `RCPT TO` list (To + Cc + Bcc).
    pub(crate) to: Vec<String>,
    /// The `EHLO` identity (the sender's domain).
    pub(crate) ehlo: String,
}

/// Derives the [`Submission`] (wire message, envelope, EHLO identity) from `draft`,
/// wrapping once through the account's MIME wrapper when one is installed and the
/// draft asks ([`Draft::sign`]/[`Draft::encrypt`]): the filed variant is wrapped and
/// the wire variant is the same ciphertext minus the `Bcc` header. Every other send
/// assembles the wire variant exactly as before. Async because the wrapper is.
pub(crate) async fn prepare_submission(
    draft: &Draft,
    mime_wrapper: Option<&Arc<dyn MimeWrapper>>,
) -> ProviderResult<Submission> {
    // One timestamp for both the transmitted and the filed copy, so they differ ONLY in
    // the Bcc header.
    let now = OffsetDateTime::now_utc();
    let prepared = prepare_message(draft, now, mime_wrapper).await?;
    let (message, filed) = match prepared {
        PreparedMessage::Plain { wire } => (wire, None),
        PreparedMessage::Wrapped { wire, filed } => (wire, Some(filed)),
    };
    let from = draft.from.email.as_str();
    // Every envelope recipient gets a `RCPT TO`: To + Cc + Bcc, de-duplicated
    // case-insensitively (the same address can appear in more than one field — e.g. To and
    // Cc) so a strict server never rejects a repeated `RCPT`. Bcc is delivered here but not
    // in the wire message's headers, so it stays hidden from the other recipients.
    let mut seen: HashSet<String> = HashSet::new();
    let to: Vec<String> = draft
        .to
        .iter()
        .chain(&draft.cc)
        .chain(&draft.bcc)
        .filter(|address| seen.insert(address.email.to_ascii_lowercase()))
        .map(|address| address.email.clone())
        .collect();
    let ehlo = from
        .rsplit_once('@')
        .map_or("localhost", |(_, domain)| domain)
        .to_owned();
    Ok(Submission {
        now,
        message,
        filed,
        from: from.to_owned(),
        to,
        ehlo,
    })
}

/// What [`prepare_message`] made of a draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PreparedMessage {
    /// No wrapper installed, or neither crypto flag set: today's exact
    /// behaviour. The filed copy is derived later (reuse when no Bcc, else a
    /// filed re-assembly).
    Plain {
        /// The over-the-wire bytes (no `Bcc` header).
        wire: Vec<u8>,
    },
    /// The wrapper ran once: both variants are its output, differing only in
    /// the `Bcc` header.
    Wrapped {
        /// The over-the-wire bytes.
        wire: Vec<u8>,
        /// The filed Sent-copy bytes.
        filed: Vec<u8>,
    },
}

/// Prepares a draft's message bytes: plain-assembled, or wrapped once when a
/// wrapper is installed and the draft asks for crypto. A refusal crosses as
/// the wrapper's own [`engine_provider::ProviderError`] (permanent for a
/// gate the user must resolve); it happens before any transport dials, so a
/// refused send never reaches the server.
pub(crate) async fn prepare_message(
    draft: &Draft,
    now: OffsetDateTime,
    wrapper: Option<&Arc<dyn MimeWrapper>>,
) -> ProviderResult<PreparedMessage> {
    let asks = draft.sign || draft.encrypt;
    if let (Some(wrapper), true) = (wrapper, asks) {
        // The filed assembly (with the `Bcc` header) is what the wrapper
        // wraps; its outcome carries both variants.
        let filed = assemble_filed_message(draft, now)?;
        let MimeWrapOutcome { wire, filed } = wrapper.wrap(draft, filed).await?;
        return Ok(PreparedMessage::Wrapped { wire, filed });
    }
    Ok(PreparedMessage::Plain {
        wire: assemble_message(draft, now)?,
    })
}

/// The filed-copy bytes for a prepared message: the wrapper's output for a
/// wrapped send, the wire copy when there is no Bcc to hide, else a filed
/// re-assembly — the same rule as before the seam.
pub(crate) fn filed_bytes(
    draft: &Draft,
    now: OffsetDateTime,
    prepared: &PreparedMessage,
) -> ProviderResult<Vec<u8>> {
    match prepared {
        PreparedMessage::Wrapped { filed, .. } => Ok(filed.clone()),
        PreparedMessage::Plain { wire } if draft.bcc.is_empty() => Ok(wire.clone()),
        PreparedMessage::Plain { .. } => Ok(assemble_filed_message(draft, now)?),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use engine_core::{ids::MessageIdHeader, mail::EmailAddress};
    use engine_provider::{MimeWrapper, ProviderError};

    use super::*;

    fn draft() -> Draft {
        Draft::new(
            MessageIdHeader::new("m1@example.test").expect("a message id"),
            EmailAddress::new("sender@example.test"),
            vec![EmailAddress::new("rcpt@example.test")],
            "the subject",
            "the body",
        )
    }

    struct Canned {
        seen: Mutex<Vec<Vec<u8>>>,
        refuse: bool,
    }

    #[async_trait::async_trait]
    impl MimeWrapper for Canned {
        async fn wrap(
            &self,
            _draft: &Draft,
            filed_message: Vec<u8>,
        ) -> Result<MimeWrapOutcome, ProviderError> {
            self.seen
                .lock()
                .expect("canned lock")
                .push(filed_message.clone());
            if self.refuse {
                return Err(ProviderError::permanent("canned refusal"));
            }
            let wire = {
                // The test double of "same ciphertext minus Bcc": strip the
                // header the way the real wrapper does.
                let text = String::from_utf8_lossy(&filed_message).to_string();
                let mut out = String::new();
                let mut in_bcc = false;
                for line in text.split("\r\n") {
                    let name = line.split(':').next().unwrap_or("").to_ascii_lowercase();
                    if !line.starts_with(' ') && !line.starts_with('\t') {
                        in_bcc = name == "bcc";
                    }
                    if !in_bcc {
                        out.push_str(line);
                        out.push_str("\r\n");
                    }
                }
                out.into_bytes()
            };
            Ok(MimeWrapOutcome {
                wire,
                filed: filed_message,
            })
        }
    }

    fn now() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("a valid instant")
    }

    #[tokio::test]
    async fn no_wrapper_assembles_the_wire_variant_exactly_as_today() {
        let prepared = prepare_message(&draft(), now(), None)
            .await
            .expect("plain prepare");
        let expected = assemble_message(&draft(), now()).expect("assemble");
        let PreparedMessage::Plain { wire } = prepared else {
            panic!("no wrapper means plain: {prepared:?}");
        };
        assert_eq!(wire, expected, "byte-identical to assemble_message");
    }

    #[tokio::test]
    async fn a_wrapper_is_consulted_only_when_the_draft_asks() {
        let canned = Arc::new(Canned {
            seen: Mutex::new(Vec::new()),
            refuse: false,
        });
        let wrapper: Arc<dyn MimeWrapper> = canned.clone();
        // Flags unset: the wrapper is NOT consulted even though installed.
        let prepared = prepare_message(&draft(), now(), Some(&wrapper))
            .await
            .expect("plain prepare");
        assert!(matches!(prepared, PreparedMessage::Plain { .. }));
        assert!(canned.seen.lock().expect("canned lock").is_empty());

        // A flag set: the wrapper sees the FILED assembly (with Bcc).
        let asking = draft()
            .with_bcc(vec![EmailAddress::new("hidden@example.test")])
            .with_crypto(true, true);
        let prepared = prepare_message(&asking, now(), Some(&wrapper))
            .await
            .expect("wrapped prepare");
        let seen = canned.seen.lock().expect("canned lock");
        assert_eq!(seen.len(), 1);
        let wrapped_input = String::from_utf8(seen[0].clone()).expect("utf-8");
        assert!(
            wrapped_input.contains("Bcc: hidden@example.test"),
            "the wrapper receives the FILED assembly: {wrapped_input}"
        );
        let PreparedMessage::Wrapped { wire, filed } = prepared else {
            panic!("a flag set means wrapped: {prepared:?}");
        };
        assert!(String::from_utf8(filed).expect("utf-8").contains("Bcc:"));
        assert!(
            !String::from_utf8(wire).expect("utf-8").contains("Bcc:"),
            "the wire variant hides the blind list"
        );
    }

    #[tokio::test]
    async fn a_refusal_crosses_as_the_wrappers_permanent_error() {
        let canned: Arc<dyn MimeWrapper> = Arc::new(Canned {
            seen: Mutex::new(Vec::new()),
            refuse: true,
        });
        let wrapper = &canned;
        let asking = draft().with_crypto(false, true);
        let err = prepare_message(&asking, now(), Some(wrapper))
            .await
            .expect_err("the refusal crosses");
        assert!(!err.class().is_retryable());
        assert!(err.detail().contains("canned refusal"));
    }

    #[test]
    fn filed_bytes_reuses_wire_only_when_unwrapped_and_bccless() {
        let bccless = draft();
        let plain = PreparedMessage::Plain {
            wire: b"wire-bytes".to_vec(),
        };
        let filed = filed_bytes(&bccless, now(), &plain).expect("filed");
        assert_eq!(filed, b"wire-bytes".to_vec());

        let bcc_draft = draft().with_bcc(vec![EmailAddress::new("hidden@example.test")]);
        let filed = filed_bytes(&bcc_draft, now(), &plain).expect("filed");
        let expected = assemble_filed_message(&bcc_draft, now()).expect("assemble filed");
        assert_eq!(filed, expected);

        let wrapped = PreparedMessage::Wrapped {
            wire: b"wire".to_vec(),
            filed: b"wrapped-filed".to_vec(),
        };
        let filed = filed_bytes(&bcc_draft, now(), &wrapped).expect("filed");
        assert_eq!(
            filed,
            b"wrapped-filed".to_vec(),
            "the wrapped copy is the wrapper's output, Bcc or not"
        );
    }
}

// The seam end to end (an installed wrapper on submit()/refile() over mock streams)
// lives in a sibling file so this one stays under the line limit.
#[cfg(test)]
#[path = "wrap_submit_tests.rs"]
mod submit_tests;
