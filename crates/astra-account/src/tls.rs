// SPDX-License-Identifier: GPL-3.0-only
//! Account-level TLS policy: the trust every connection of an account runs under, and
//! the certificate exceptions its owner accepted pinned into the stored config.
//!
//! Native clients use the Firefox-style trust policy (bundled Mozilla roots plus the OS
//! store). On top of that policy an account carries the certificates its owner accepted
//! for a server that could not be verified. Those never widen the policy: the verifier
//! consults them only once it has already refused, and each admits one certificate on
//! one server name.

use engine_tls::{
    client_config_with_exceptions, CertificateDer, TlsClientConfig, TlsError, TlsPolicy,
};
use serde::{Deserialize, Serialize};

use crate::AccountConfig;

/// Builds the shared TLS config for one account's connections, honouring the
/// certificates its owner accepted.
///
/// # Errors
///
/// Returns [`TlsError`] when the trust anchors cannot be assembled into a config.
pub fn account_tls(account: &AccountConfig) -> Result<TlsClientConfig, TlsError> {
    client_config_with_exceptions(&setup_trust_policy(), &account.tls_exceptions())
}

/// The trust policy, with no exceptions, for the dials a setup makes before an account
/// exists: the connection test a setup screen runs against fields nobody has saved yet.
///
/// The same policy the account's own connections get, so one place decides the trust
/// every connection of this product runs under.
#[must_use]
pub fn setup_trust_policy() -> TlsPolicy {
    TlsPolicy::bundled_and_system()
}

/// The length of a SHA-256 fingerprint written as hex, without separators.
const FINGERPRINT_HEX_LEN: usize = 64;

/// A server certificate accepted for this account although it did not verify.
///
/// Both fields are as stored: `server_name` matches the account's own
/// `imap.server_name` / `smtp.server_name`, and `sha256` is lowercase hex of the
/// certificate's DER. What is kept is the SHA-256 of that one certificate, scoped to
/// the one TLS server name, so the exception is worth nothing to any other server and
/// nothing to that server presenting anything else; `engine-tls` enforces it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificateException {
    /// The TLS server name the exception is scoped to.
    pub server_name: String,
    /// SHA-256 of the accepted certificate's DER, lowercase hex.
    pub sha256: String,
}

impl CertificateException {
    /// The exception accepting `certificate` for `server_name`.
    #[must_use]
    pub fn new(server_name: &str, certificate: &CertificateDer<'_>) -> Self {
        Self {
            server_name: server_name.trim().to_lowercase(),
            sha256: hex(&engine_tls::fingerprint(certificate)),
        }
    }

    /// The exception accepting the certificate with fingerprint `sha256` for
    /// `server_name`: the form a client hands back after somebody accepted what it was
    /// shown.
    ///
    /// `sha256` is read in either form, `:`-separated or not and in any case, and stored
    /// canonically. `None` when it is not a SHA-256 fingerprint at all, so nothing
    /// unreadable is ever written to the config.
    #[must_use]
    pub fn accepted(server_name: &str, sha256: &str) -> Option<Self> {
        Some(Self {
            server_name: server_name.trim().to_lowercase(),
            sha256: hex(&parse_fingerprint(sha256)?),
        })
    }

    /// This exception as the table the stored config carries it in.
    pub(crate) fn to_table(&self) -> toml::Table {
        let mut table = toml::Table::new();
        table.insert("server_name".into(), self.server_name.clone().into());
        table.insert("sha256".into(), self.sha256.clone().into());
        table
    }

    /// The engine form, or `None` when `sha256` is not a SHA-256 fingerprint.
    ///
    /// A stored exception nobody can read is dropped rather than refused: the connect
    /// it would have permitted then fails with the certificate error it always did,
    /// which asks the person for the exception again. Refusing to load the account
    /// instead would lock them out of a mailbox over a file they never see.
    pub(crate) fn to_engine(&self) -> Option<engine_tls::CertificateException> {
        Some(engine_tls::CertificateException::from_fingerprint(
            &self.server_name,
            parse_fingerprint(&self.sha256)?,
        ))
    }
}

/// A fingerprint as lowercase hex, the form the stored config carries.
fn hex(fingerprint: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    fingerprint.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Reads a fingerprint back from either form: `:`-separated or not, any case.
fn parse_fingerprint(text: &str) -> Option<[u8; 32]> {
    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| *byte != b':' && !byte.is_ascii_whitespace())
        .collect();
    if digits.len() != FINGERPRINT_HEX_LEN {
        return None;
    }
    let mut fingerprint = [0u8; 32];
    let (pairs, _) = digits.as_chunks::<2>();
    for (byte, pair) in fingerprint.iter_mut().zip(pairs) {
        let pair = std::str::from_utf8(pair).ok()?;
        *byte = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(fingerprint)
}

#[cfg(test)]
mod tests {
    use super::{hex, parse_fingerprint, CertificateDer, CertificateException};

    fn certificate() -> CertificateDer<'static> {
        CertificateDer::from(b"a certificate".to_vec())
    }

    /// The form every tool displays a fingerprint in: uppercase and colon-separated.
    fn displayed(fingerprint: &[u8; 32]) -> String {
        fingerprint
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }

    #[test]
    fn an_exception_carries_the_certificates_fingerprint() {
        let exception = CertificateException::new("MAIL.Example.com ", &certificate());

        assert_eq!(exception.server_name, "mail.example.com");
        assert_eq!(
            exception.sha256,
            hex(&engine_tls::fingerprint(&certificate()))
        );
        assert_eq!(exception.sha256.len(), 64);
    }

    #[test]
    fn a_stored_exception_reaches_the_engine_in_the_same_shape() {
        let stored = CertificateException::new("mail.example.com", &certificate());
        let engine = stored.to_engine().expect("a well-formed fingerprint");

        assert_eq!(engine.server_name(), "mail.example.com");
        assert_eq!(
            engine.fingerprint(),
            engine_tls::fingerprint(&certificate())
        );
    }

    /// What a client shows is what it may hand back, so the displayed form is accepted
    /// and canonicalised rather than stored as it arrived.
    #[test]
    fn an_acceptance_takes_the_fingerprint_in_the_form_it_was_displayed_in() {
        let shown = displayed(&engine_tls::fingerprint(&certificate()));
        let accepted =
            CertificateException::accepted("mail.example.com", &shown).expect("a fingerprint");

        assert_eq!(
            accepted,
            CertificateException::new("mail.example.com", &certificate())
        );
        assert!(!accepted.sha256.contains(':'));
    }

    #[test]
    fn an_unreadable_fingerprint_is_never_accepted_or_stored() {
        for text in ["", "not hex", &"ab".repeat(31), &"zz".repeat(32)] {
            assert!(
                CertificateException::accepted("mail.example.com", text).is_none(),
                "{text:?} is not a fingerprint"
            );
            assert!(
                parse_fingerprint(text).is_none(),
                "{text:?} is not a fingerprint"
            );
        }
    }

    /// An exception that somehow reached the config unreadable drops out rather than
    /// taking the account down with it.
    #[test]
    fn an_unreadable_stored_exception_drops_rather_than_failing_the_account() {
        let exception = CertificateException {
            server_name: "mail.example.com".to_owned(),
            sha256: "not a fingerprint".to_owned(),
        };
        assert!(exception.to_engine().is_none());
    }

    #[test]
    fn a_displayed_fingerprint_reads_back_as_the_same_bytes() {
        let fingerprint = engine_tls::fingerprint(&certificate());
        let shown = displayed(&fingerprint);

        assert_eq!(shown.matches(':').count(), 31);
        assert_eq!(parse_fingerprint(&shown), Some(fingerprint));
        assert_eq!(parse_fingerprint(&hex(&fingerprint)), Some(fingerprint));
    }
}
