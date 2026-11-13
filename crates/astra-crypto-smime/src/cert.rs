// SPDX-License-Identifier: GPL-3.0-only
//! Self-signed S/MIME certificate construction over the x509-cert 0.3 stack.
//!
//! Plan 2 Task 4: build a v3 self-signed S/MIME leaf certificate over a fresh
//! ECDSA P-256 keypair, set the S/MIME-relevant extensions (KeyUsage, EKU
//! `emailProtection`, SAN `rfc822Name`, SubjectKeyIdentifier), and DER-encode
//! both the certificate and the PKCS#8 private key.
//!
//! API references (local RustCrypto source, `formats/x509-cert@v0.3.0`):
//! - `builder.rs`: `CertificateBuilder::new(profile, serial, validity, spki)`,
//!   `Builder::build::<_, DerSignature>(&signer)`, `BuilderProfile` trait.
//! - `ext/pkix/keyusage.rs`: `KeyUsage(FlagSet<KeyUsages>)`, `ExtendedKeyUsage(Vec<OID>)`.
//! - `ext/pkix.rs`: `SubjectAltName(GeneralNames)`, `SubjectKeyIdentifier(OctetString)`.
//! - `ext/pkix/name/general.rs`: `GeneralName::Rfc822Name(Ia5String)`.
//! - `tests/builder.rs::ecdsa_signer`: `p256::ecdsa::SigningKey` + `generate_from_rng`.

use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use der::asn1::{Ia5String, ObjectIdentifier};
use der::referenced::OwnedToRef;
use der::Encode;
use p256::ecdsa::{DerSignature, SigningKey};
use p256::elliptic_curve::Generate;
use pkcs8::EncodePrivateKey;
use x509_cert::builder::profile::BuilderProfile;
use x509_cert::builder::{Builder, CertificateBuilder};
// `x509_cert::SubjectPublicKeyInfo` is the concrete `SubjectPublicKeyInfoOwned`
// alias (spki 0.8 made the struct generic `<Params, Key>`; x509-cert pins the
// owned `Any`/`BitString` instantiation that `CertificateBuilder::new` expects).
use x509_cert::certificate::TbsCertificate;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::{
    ExtendedKeyUsage, KeyUsage, KeyUsages, SubjectAltName, SubjectKeyIdentifier,
};
use x509_cert::ext::Extension;
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::Validity;
use x509_cert::SubjectPublicKeyInfo;

use astra_crypto_core::{CryptoError, Result as CcResult};

/// id-kp-emailProtection (1.3.6.1.5.5.7.3.4) - RFC 5280.
const EKU_EMAIL_PROTECTION: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.4");

/// A minimal self-signed S/MIME *leaf* profile: `issuer == subject` (self-signed),
/// and it contributes no default extensions - every extension is added explicitly
/// by [`build_self_signed_smime_cert`] so the S/MIME KeyUsage/EKU/SAN are exactly
/// what we set (the built-in `cabf::Root` profile would force CA-oriented usage).
pub(crate) struct SmimeLeafProfile {
    subject: Name,
}

impl BuilderProfile for SmimeLeafProfile {
    fn get_subject(&self) -> Name {
        self.subject.clone()
    }

    fn get_issuer(&self, subject: &Name) -> Name {
        // Self-signed: the issuer is the subject.
        subject.clone()
    }

    fn build_extensions(
        &self,
        _spk: spki::SubjectPublicKeyInfoRef<'_>,
        _issuer_spk: spki::SubjectPublicKeyInfoRef<'_>,
        _tbs: &TbsCertificate,
    ) -> x509_cert::builder::Result<Vec<Extension>> {
        Ok(Vec::new())
    }
}

/// Output of building a self-signed S/MIME cert + keypair.
pub(crate) struct BuiltCert {
    /// DER-encoded X.509 v3 self-signed certificate.
    pub cert_der: Vec<u8>,
    /// Unencrypted PKCS#8 DER private key (ECDSA P-256).
    pub priv_pkcs8_der: Vec<u8>,
    /// Lowercase hex of the certificate SubjectKeyIdentifier (RFC 5280 method 1:
    /// SHA-1 of the `SubjectPublicKeyInfo`). Used as the `Fingerprint`.
    pub ski_hex: String,
}

fn map_err<E: std::fmt::Display>(ctx: &str, e: E) -> CryptoError {
    CryptoError::Malformed(format!("smime cert: {ctx}: {e}"))
}

/// Build a self-signed v3 S/MIME certificate over a fresh ECDSA P-256 keypair.
///
/// Extensions set:
/// - **KeyUsage** (`digitalSignature`, `keyEncipherment`)
/// - **ExtendedKeyUsage** (`id-kp-emailProtection`)
/// - **SubjectAltName** (`rfc822Name = <email>`)
/// - **SubjectKeyIdentifier** (RFC 5280 method 1; also returned as `ski_hex`)
///
/// Validity is 1 year from now. `email` is also reflected in the Subject DN as
/// `CN = <local-part>` (the email itself lives in the SAN per S/MIME practice).
pub(crate) fn build_self_signed_smime_cert(email: &str) -> CcResult<BuiltCert> {
    // --- keypair (ECDSA P-256) ---
    let mut rng = rand::rng();
    let signing_key = SigningKey::generate_from_rng(&mut rng);
    let verifying_key = signing_key.verifying_key();
    let pub_spki =
        SubjectPublicKeyInfo::from_key(verifying_key).map_err(|e| map_err("spki from key", e))?;

    // --- SubjectKeyIdentifier (also used as the cert fingerprint) ---
    let ski = SubjectKeyIdentifier::try_from(pub_spki.owned_to_ref())
        .map_err(|e| map_err("subject key id", e))?;
    let ski_hex = to_hex_lower(ski.0.as_bytes());

    // --- subject DN: CN = <local-part> (keep '@' out of the DN) ---
    let cn = email
        .split('@')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("smime-key");
    let subject = Name::from_str(&format!("CN={cn}")).map_err(|e| map_err("subject name", e))?;

    // --- serial (time-based, monotonically varying) + validity (1 year) ---
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1);
    let serial = SerialNumber::from(secs as u32);
    let validity = Validity::from_now(Duration::from_secs(365 * 24 * 60 * 60))
        .map_err(|e| map_err("validity", e))?;

    // --- builder + extensions ---
    let profile = SmimeLeafProfile { subject };
    let mut builder = CertificateBuilder::new(profile, serial, validity, pub_spki)
        .map_err(|e| map_err("cert builder", e))?;

    // ToExtension is implemented for `&T` (x509-cert 0.3), so we pass each
    // extension by reference.

    // KeyUsage: digitalSignature + keyEncipherment (S/MIME signing + key transport).
    let key_usage = KeyUsage(KeyUsages::DigitalSignature | KeyUsages::KeyEncipherment);
    builder
        .add_extension(&key_usage)
        .map_err(|e| map_err("key usage ext", e))?;

    // EKU: emailProtection.
    let eku = ExtendedKeyUsage(vec![EKU_EMAIL_PROTECTION]);
    builder
        .add_extension(&eku)
        .map_err(|e| map_err("eku ext", e))?;

    // SAN: rfc822Name = <email>.
    let email_ia5 = Ia5String::new(email.as_bytes()).map_err(|e| map_err("san email", e))?;
    let san = SubjectAltName(vec![GeneralName::Rfc822Name(email_ia5)]);
    builder
        .add_extension(&san)
        .map_err(|e| map_err("san ext", e))?;

    // SubjectKeyIdentifier.
    builder
        .add_extension(&ski)
        .map_err(|e| map_err("ski ext", e))?;

    // --- sign with ECDSA P-256 (DER-encoded signature) ---
    let cert = builder
        .build::<_, DerSignature>(&signing_key)
        .map_err(|e| map_err("cert build/sign", e))?;
    let cert_der = cert.to_der().map_err(|e| map_err("cert to_der", e))?;

    // --- PKCS#8 DER private key (unencrypted; passphrase handled by the keystore's at-rest layer) ---
    let priv_doc = signing_key
        .to_pkcs8_der()
        .map_err(|e| map_err("pkcs8 der", e))?;
    let priv_pkcs8_der = priv_doc.as_bytes().to_vec();

    Ok(BuiltCert {
        cert_der,
        priv_pkcs8_der,
        ski_hex,
    })
}

pub(crate) fn to_hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// RFC 5280 method-1 SubjectKeyIdentifier = SHA-1 of the SPKI DER, hex-lower.
/// The SAME computation `build_self_signed_smime_cert` uses to derive a cert's
/// `Fingerprint`, factored out so:
///
/// - `persist_imported` (leaf) and `import_p12_with_chain` (intermediates)
///   compute identical fingerprints for the same cert (re-import dedup).
/// - the backend receive path (`run_verify_path`) can dedup the union of
///   SignedData intermediates + stored intermediates by the same key.
/// - the backend persistence layer (`upsert_intermediate_cert`) computes the
///   `(account_id, standard, fingerprint)` UNIQUE key with the same algorithm
///   `SqliteKeyStore::put` derives for the leaf, so an intermediate that later
///   appears in a `.p12` bag overwrites the same row (not a duplicate).
///
/// Returns `Err(Malformed)` if `cert_der` is not a parseable X.509 DER cert or
/// the SPKI lacks a `SubjectKeyIdentifier` extension derivation.
pub fn fingerprint_of_cert_der(cert_der: &[u8]) -> CcResult<String> {
    let cert = <x509_cert::Certificate as der::Decode>::from_der(cert_der)
        .map_err(|e| CryptoError::Malformed(format!("parse cert DER: {e}")))?;
    let spki_ref = cert
        .tbs_certificate()
        .subject_public_key_info()
        .owned_to_ref();
    let ski = SubjectKeyIdentifier::try_from(spki_ref)
        .map_err(|e| CryptoError::Malformed(format!("compute SKI: {e}")))?;
    Ok(to_hex_lower(ski.0.as_bytes()))
}

/// Human-facing metadata extracted from an X.509 certificate. DISPLAY ONLY -
/// no trust/validation semantics (those live in `chain.rs`). Powers the Key
/// Manager's subject/expiry row display and the token key picker's per-key
/// cert summary, so users see "CN=John Doe · valid until 2027-03-11" instead
/// of a bare serial/thumbprint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CertSummary {
    /// Subject CN (first one), when present.
    pub subject_cn: Option<String>,
    /// SAN `rfc822Name` (modern S/MIME), falling back to the legacy subject-DN
    /// `emailAddress` attribute.
    pub email: Option<String>,
    /// Issuer CN (first one), when present.
    pub issuer_cn: Option<String>,
    /// Lowercase hex of the cert serial number.
    pub serial_hex: String,
    pub not_before_unix: i64,
    pub not_after_unix: i64,
    /// SPKI display label: "RSA-2048", "ECDSA-P256", "Ed25519", …
    pub public_key_algorithm: String,
    /// SPKI algorithm OID (dotted string, e.g. "1.2.840.113549.1.1.1") -
    /// the signature-details dialog maps OIDs to labels itself.
    pub public_key_algorithm_oid: String,
}

/// Extract display metadata from a DER-encoded X.509 certificate via
/// x509-parser (the crate's read-path parser). Returns `Err(Malformed)` for
/// anything that is not a cert - callers treat that as "no summary" (OpenPGP
/// key material and token blobs without a cert flow through the same paths).
pub fn cert_summary_of_der(cert_der: &[u8]) -> CcResult<CertSummary> {
    let (_rem, cert) = x509_parser::parse_x509_certificate(cert_der)
        .map_err(|e| CryptoError::Malformed(format!("parse cert DER: {e}")))?;

    let subject_cn = cert
        .subject()
        .iter_common_name()
        .find_map(|a| a.as_str().ok().map(str::to_string));
    let issuer_cn = cert
        .issuer()
        .iter_common_name()
        .find_map(|a| a.as_str().ok().map(str::to_string));

    // Email: SAN rfc822Name wins; the subject-DN emailAddress attr is the
    // legacy fallback (older enterprise CAs still carry it).
    let email = cert
        .subject_alternative_name()
        .ok()
        .flatten()
        .and_then(|san| {
            san.value.general_names.iter().find_map(|gn| match gn {
                x509_parser::extensions::GeneralName::RFC822Name(s) => Some((*s).to_string()),
                _ => None,
            })
        })
        .or_else(|| {
            cert.subject()
                .iter_email()
                .find_map(|a| a.as_str().ok().map(str::to_string))
        });

    let validity = cert.validity();
    Ok(CertSummary {
        subject_cn,
        email,
        issuer_cn,
        serial_hex: to_hex_lower(&cert.serial.to_bytes_be()),
        not_before_unix: validity.not_before.timestamp(),
        not_after_unix: validity.not_after.timestamp(),
        public_key_algorithm: spki_display(&cert),
        public_key_algorithm_oid: cert.public_key().algorithm.algorithm.to_string(),
    })
}

/// SPKI display label: "RSA-2048" from the parsed modulus length, curve-size
/// labels for EC, Ed25519/X25519 from the algorithm OID (their SPKI carries
/// no curve parameters).
fn spki_display(cert: &x509_parser::certificate::X509Certificate<'_>) -> String {
    let spki = cert.public_key();
    match spki.algorithm.algorithm.to_id_string().as_str() {
        "1.3.101.112" => return "Ed25519".to_string(),
        "1.3.101.110" => return "X25519".to_string(),
        _ => {}
    }
    match spki.parsed() {
        Ok(pk @ x509_parser::public_key::PublicKey::RSA(_)) => {
            format!("RSA-{}", pk.key_size())
        }
        Ok(pk @ x509_parser::public_key::PublicKey::EC(_)) => match pk.key_size() {
            256 => "ECDSA-P256".to_string(),
            384 => "ECDSA-P384".to_string(),
            521 => "ECDSA-P521".to_string(),
            _ => "EC".to_string(),
        },
        _ => "unknown".to_string(),
    }
}

/// Extract the CRL distribution-point URLs from a DER-encoded X.509 certificate
/// (SP4c-3b D1). Reads the `cRLDistributionPoints` extension via x509-parser and
/// collects every `fullName` `uniformResourceIdentifier` general name. Only
/// `http` and `https` URLs are admissible: a distribution-point URL is
/// attacker-influenced (it comes from the cert), so any other scheme is dropped.
/// URLs are deduplicated, preserving first-seen order. A cert with no
/// distribution-point extension yields an empty list. Returns `Err(Malformed)`
/// for anything that is not a parseable X.509 cert.
pub fn crl_distribution_point_urls(cert_der: &[u8]) -> CcResult<Vec<String>> {
    let (_rem, cert) = x509_parser::parse_x509_certificate(cert_der)
        .map_err(|e| CryptoError::Malformed(format!("parse cert DER: {e}")))?;
    let ext = match cert
        .get_extension_unique(&x509_parser::oid_registry::OID_X509_EXT_CRL_DISTRIBUTION_POINTS)
    {
        Ok(Some(ext)) => ext,
        _ => return Ok(Vec::new()),
    };
    let points = match ext.parsed_extension() {
        x509_parser::extensions::ParsedExtension::CRLDistributionPoints(points) => points,
        _ => return Ok(Vec::new()),
    };
    let mut urls: Vec<String> = Vec::new();
    for point in points.iter() {
        let Some(x509_parser::extensions::DistributionPointName::FullName(names)) =
            &point.distribution_point
        else {
            continue;
        };
        for name in names {
            if let x509_parser::extensions::GeneralName::URI(uri) = name {
                if is_http_url(uri) && !urls.iter().any(|u| u == uri) {
                    urls.push((*uri).to_string());
                }
            }
        }
    }
    Ok(urls)
}

/// Whether a distribution-point URL uses an admissible `http` or `https` scheme
/// (case-insensitive, per RFC 3986). Anything else (ldap, ftp, file, a bare
/// string) is rejected.
fn is_http_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    !rest.is_empty() && matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
}

/// The cache-relevant metadata parsed from a CRL's DER (SP4c-3b): the issuing
/// CA's distinguished name (for display) and the `nextUpdate` instant as an
/// epoch-seconds string (the cache's freshness key). Parsing lives here in the
/// smime crate, never the store; the receive fetch-or-cache layer calls it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrlMetadata {
    /// The CRL issuer's distinguished name (RFC 4514 string), when present.
    pub issuer_dn: Option<String>,
    /// The `nextUpdate` time as epoch seconds, when the CRL carries one.
    pub next_update: Option<String>,
}

/// Parse the issuer DN and `nextUpdate` from a DER-encoded CRL via x509-parser.
/// Returns `Err(Malformed)` for anything that is not a parseable CRL; the caller
/// treats that as "no usable CRL" and degrades to unchecked (fail-soft).
pub fn crl_metadata_of_der(crl_der: &[u8]) -> CcResult<CrlMetadata> {
    let (_rem, crl) = x509_parser::parse_x509_crl(crl_der)
        .map_err(|e| CryptoError::Malformed(format!("parse CRL DER: {e}")))?;
    Ok(CrlMetadata {
        issuer_dn: Some(crl.issuer().to_string()),
        next_update: crl.next_update().map(|t| t.timestamp().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_extracts_cn_email_validity_and_ecdsa_label() {
        let built = build_self_signed_smime_cert("alice@kylins.com").expect("build");
        let s = cert_summary_of_der(&built.cert_der).expect("summary");
        assert_eq!(s.subject_cn.as_deref(), Some("alice"));
        // Self-signed: issuer == subject.
        assert_eq!(s.issuer_cn.as_deref(), Some("alice"));
        assert_eq!(s.email.as_deref(), Some("alice@kylins.com"));
        assert_eq!(s.public_key_algorithm, "ECDSA-P256");
        assert!(
            s.not_after_unix - s.not_before_unix >= 364 * 24 * 3600,
            "validity ~1 year: {} -> {}",
            s.not_before_unix,
            s.not_after_unix
        );
        assert!(!s.serial_hex.is_empty());
        // The same cert still SKI-fingerprints identically.
        assert_eq!(
            fingerprint_of_cert_der(&built.cert_der).unwrap(),
            built.ski_hex
        );
    }

    #[test]
    fn summary_labels_rsa_key_size() {
        // Build an RSA-2048 self-signed cert through the same x509-cert 0.3
        // builder (RSA signs with PKCS#1 v1.5 + SHA-256).
        let mut rng = rand::rng();
        let priv_key = rsa::RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
        let pub_spki = SubjectPublicKeyInfo::from_key(&priv_key.to_public_key()).expect("rsa spki");
        let signing_key = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(priv_key);

        let subject = Name::from_str("CN=rsa-test").expect("subject");
        let profile = SmimeLeafProfile { subject };
        let serial = SerialNumber::from(42u32);
        let validity = Validity::from_now(Duration::from_secs(3600)).expect("validity");
        let mut builder =
            CertificateBuilder::new(profile, serial, validity, pub_spki).expect("builder");
        let san = SubjectAltName(vec![GeneralName::Rfc822Name(
            Ia5String::new(b"rsa@x.com").expect("san ia5"),
        )]);
        builder.add_extension(&san).expect("san ext");
        let cert = builder
            .build::<_, rsa::pkcs1v15::Signature>(&signing_key)
            .expect("rsa cert build");
        let der = cert.to_der().expect("to_der");

        let s = cert_summary_of_der(&der).expect("summary");
        assert_eq!(s.public_key_algorithm, "RSA-2048");
        assert_eq!(s.subject_cn.as_deref(), Some("rsa-test"));
        assert_eq!(s.email.as_deref(), Some("rsa@x.com"));
    }

    #[test]
    fn summary_rejects_non_x509_blob() {
        assert!(cert_summary_of_der(b"-----BEGIN PGP PUBLIC KEY BLOCK-----").is_err());
        assert!(cert_summary_of_der(b"").is_err());
    }

    // --- SP4c-3b: CRL distribution-point extraction ---

    use x509_cert::ext::pkix::crl::dp::DistributionPoint;
    use x509_cert::ext::pkix::name::DistributionPointName;
    use x509_cert::ext::pkix::CrlDistributionPoints;

    /// Build a self-signed cert carrying a `cRLDistributionPoints` extension
    /// with one `fullName` URI per entry of `urls`. An empty slice builds a
    /// cert with no distribution-point extension at all.
    fn cert_with_dp_urls(urls: &[&str]) -> Vec<u8> {
        let mut rng = rand::rng();
        let signing_key = SigningKey::generate_from_rng(&mut rng);
        let pub_spki = SubjectPublicKeyInfo::from_key(signing_key.verifying_key()).expect("spki");
        let subject = Name::from_str("CN=dp-test").expect("subject");
        let profile = SmimeLeafProfile { subject };
        let serial = SerialNumber::from(7u32);
        let validity = Validity::from_now(Duration::from_secs(3600)).expect("validity");
        let mut builder =
            CertificateBuilder::new(profile, serial, validity, pub_spki).expect("builder");
        if !urls.is_empty() {
            let names: Vec<GeneralName> = urls
                .iter()
                .map(|u| {
                    GeneralName::UniformResourceIdentifier(
                        Ia5String::new(u.as_bytes()).expect("uri ia5"),
                    )
                })
                .collect();
            let dp = DistributionPoint {
                distribution_point: Some(DistributionPointName::FullName(names)),
                reasons: None,
                crl_issuer: None,
            };
            builder
                .add_extension(&CrlDistributionPoints(vec![dp]))
                .expect("dp ext");
        }
        let cert = builder
            .build::<_, DerSignature>(&signing_key)
            .expect("build");
        cert.to_der().expect("to_der")
    }

    #[test]
    fn dp_extraction_yields_the_http_url() {
        let der = cert_with_dp_urls(&["http://ca.example.com/crl.der"]);
        let urls = crl_distribution_point_urls(&der).expect("extract");
        assert_eq!(urls, vec!["http://ca.example.com/crl.der".to_string()]);
    }

    #[test]
    fn dp_extraction_yields_https_and_dedups_preserving_order() {
        let der = cert_with_dp_urls(&[
            "https://ca.example.com/a.crl",
            "http://ca.example.com/b.crl",
            "https://ca.example.com/a.crl",
        ]);
        let urls = crl_distribution_point_urls(&der).expect("extract");
        assert_eq!(
            urls,
            vec![
                "https://ca.example.com/a.crl".to_string(),
                "http://ca.example.com/b.crl".to_string(),
            ]
        );
    }

    #[test]
    fn dp_extraction_without_the_extension_yields_nothing() {
        let der = cert_with_dp_urls(&[]);
        assert!(crl_distribution_point_urls(&der)
            .expect("extract")
            .is_empty());
        // A production self-signed leaf carries no DP extension either.
        let built = build_self_signed_smime_cert("alice@kylins.com").expect("build");
        assert!(crl_distribution_point_urls(&built.cert_der)
            .expect("extract")
            .is_empty());
    }

    #[test]
    fn dp_extraction_rejects_non_http_schemes() {
        let der = cert_with_dp_urls(&[
            "ldap://ca.example.com/crl",
            "ftp://ca.example.com/crl",
            "file:///etc/crl",
            "http://ok.example.com/crl.der",
        ]);
        let urls = crl_distribution_point_urls(&der).expect("extract");
        assert_eq!(urls, vec!["http://ok.example.com/crl.der".to_string()]);
    }

    #[test]
    fn dp_extraction_accepts_an_uppercase_scheme() {
        let der = cert_with_dp_urls(&["HTTPS://ca.example.com/crl.der"]);
        let urls = crl_distribution_point_urls(&der).expect("extract");
        assert_eq!(urls, vec!["HTTPS://ca.example.com/crl.der".to_string()]);
    }

    #[test]
    fn dp_extraction_rejects_a_non_cert_blob() {
        assert!(crl_distribution_point_urls(b"not a cert").is_err());
        assert!(crl_distribution_point_urls(b"").is_err());
    }

    #[test]
    fn crl_metadata_rejects_a_non_crl_blob() {
        assert!(crl_metadata_of_der(b"not a crl").is_err());
        assert!(crl_metadata_of_der(b"").is_err());
    }
}
