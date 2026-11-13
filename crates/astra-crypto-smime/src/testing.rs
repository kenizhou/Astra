// SPDX-License-Identifier: GPL-3.0-only
//! Cross-crate test helpers (backend integration tests).
//!
//! **NOT part of the public API.** Exposed under `#[doc(hidden)]` + gated by
//! the `testing` Cargo feature so backend tests (`kylins.client.backend/tests`
//! + `mail/crypto.rs::tests`) can build real signed cert chains + SignedData
//! blobs without duplicating the x509-cert 0.3 builder stack (the backend
//! crate does not depend on `x509-cert` / `p256` / `pkcs8` directly).
//!
//! Enable via `features = ["testing"]` in `[dev-dependencies]`. The helpers
//! are pure test-fixture builders - no production caller should reach them
//! (the feature is not in the default set).

#[cfg(feature = "testing")]
mod inner {
    use std::str::FromStr;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use der::referenced::OwnedToRef;
    use der::Encode;
    use p256::ecdsa::{DerSignature, SigningKey};
    use p256::elliptic_curve::Generate;
    use p256::pkcs8::DecodePrivateKey;
    use pkcs8::EncodePrivateKey;
    use x509_cert::builder::profile::BuilderProfile;
    use x509_cert::builder::{Builder, CertificateBuilder};
    use x509_cert::certificate::TbsCertificate;
    use x509_cert::ext::pkix::name::GeneralName;
    use x509_cert::ext::pkix::{
        BasicConstraints, ExtendedKeyUsage, KeyUsage, KeyUsages, SubjectAltName,
        SubjectKeyIdentifier,
    };
    use x509_cert::ext::Extension;
    use x509_cert::name::Name;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::time::Validity;
    use x509_cert::SubjectPublicKeyInfo;

    /// id-kp-emailProtection (1.3.6.1.5.5.7.3.4) - RFC 5280.
    const EKU_EMAIL_PROTECTION: der::asn1::ObjectIdentifier =
        der::asn1::ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.4");

    /// Validity window for the test certs. Kept well under SmimeProfile's
    /// 825-day cap (the receive-path pkix-profiles-cabf SmimeProfile enforces
    /// `max_validity_secs` on EVERY cert in the chain, root included).
    const TEST_VALIDITY_SECS: u64 = 200 * 24 * 60 * 60;

    /// Built cert + private key (PKCS#8 DER) for the test fixture.
    #[doc(hidden)]
    #[derive(Clone)]
    pub struct BuiltCert {
        /// X.509 v3 cert DER.
        pub cert_der: Vec<u8>,
        /// PKCS#8 DER private key (unencrypted).
        pub priv_pkcs8_der: Vec<u8>,
    }

    /// A test profile parameterized on subject + issuer (self-signed when they
    /// coincide). Returns no default extensions; every extension is added
    /// explicitly by the caller so the leaf vs CA shape is fully controlled.
    struct TestProfile {
        subject: Name,
        issuer: Name,
    }

    impl BuilderProfile for TestProfile {
        fn get_subject(&self) -> Name {
            self.subject.clone()
        }
        fn get_issuer(&self, _subject: &Name) -> Name {
            // Ignore the implicit `subject` arg - CA-signed certs carry their
            // own issuer (the parent CA's subject).
            self.issuer.clone()
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

    /// Build a self-signed CA cert (BasicConstraints cA:TRUE,
    /// KeyUsage keyCertSign | cRLSign, SubjectKeyIdentifier) over a fresh
    /// ECDSA P-256 keypair. Use as the test "intermediate" or "root" - chain
    /// validation treats it as a CA.
    #[doc(hidden)]
    pub fn build_self_signed_ca(cn: &str) -> BuiltCert {
        let mut rng = rand::rng();
        let signing_key = SigningKey::generate_from_rng(&mut rng);
        let verifying_key = signing_key.verifying_key();
        let pub_spki = SubjectPublicKeyInfo::from_key(verifying_key).expect("ca spki");
        let ski = SubjectKeyIdentifier::try_from(pub_spki.owned_to_ref()).expect("ca ski");

        let subject = Name::from_str(&format!("CN={cn}")).expect("ca subject");
        let profile = TestProfile {
            subject: subject.clone(),
            issuer: subject,
        };
        let secs = now_secs();
        let serial = SerialNumber::from(secs as u32);
        let validity = Validity::from_now(Duration::from_secs(TEST_VALIDITY_SECS)).expect("ca val");

        let mut builder =
            CertificateBuilder::new(profile, serial, validity, pub_spki).expect("ca builder");
        builder
            .add_extension(&BasicConstraints {
                ca: true,
                path_len_constraint: None,
            })
            .expect("ca bc");
        builder
            .add_extension(&KeyUsage(KeyUsages::KeyCertSign | KeyUsages::CRLSign))
            .expect("ca ku");
        builder.add_extension(&ski).expect("ca ski");

        let cert = builder
            .build::<_, DerSignature>(&signing_key)
            .expect("ca build");
        let cert_der = cert.to_der().expect("ca to_der");
        let priv_pkcs8_der = signing_key
            .to_pkcs8_der()
            .expect("ca pkcs8")
            .as_bytes()
            .to_vec();
        BuiltCert {
            cert_der,
            priv_pkcs8_der,
        }
    }

    /// Build an intermediate CA cert SIGNED BY `parent` (BasicConstraints
    /// cA:TRUE, KeyUsage keyCertSign | cRLSign, SubjectKeyIdentifier). Use to
    /// build a 3-cert chain: root (self-signed) → intermediate (signed by
    /// root) → leaf (signed by intermediate). The intermediate is the cert
    /// stored as `key_type='intermediate'` in the receive-path merge test -
    /// without it, the chain cannot link leaf → root.
    #[doc(hidden)]
    pub fn build_intermediate_signed_by(cn: &str, parent: &BuiltCert) -> BuiltCert {
        let mut rng = rand::rng();
        let signing_key = SigningKey::generate_from_rng(&mut rng);
        let verifying_key = signing_key.verifying_key();
        let pub_spki = SubjectPublicKeyInfo::from_key(verifying_key).expect("inter spki");
        let ski = SubjectKeyIdentifier::try_from(pub_spki.owned_to_ref()).expect("inter ski");

        let subject = Name::from_str(&format!("CN={cn}")).expect("inter subject");
        let parent_cert = <x509_cert::Certificate as der::Decode>::from_der(&parent.cert_der)
            .expect("parse parent");
        let issuer = parent_cert.tbs_certificate().subject().clone();
        let profile = TestProfile { subject, issuer };

        let secs = now_secs();
        let serial = SerialNumber::from(secs as u32);
        let validity =
            Validity::from_now(Duration::from_secs(TEST_VALIDITY_SECS)).expect("inter val");

        let mut builder =
            CertificateBuilder::new(profile, serial, validity, pub_spki).expect("inter builder");
        builder
            .add_extension(&BasicConstraints {
                ca: true,
                path_len_constraint: None,
            })
            .expect("inter bc");
        builder
            .add_extension(&KeyUsage(KeyUsages::KeyCertSign | KeyUsages::CRLSign))
            .expect("inter ku");
        builder.add_extension(&ski).expect("inter ski");

        let parent_sk = SigningKey::from_pkcs8_der(&parent.priv_pkcs8_der).expect("parent key");
        let cert = builder
            .build::<_, DerSignature>(&parent_sk)
            .expect("inter build");
        let cert_der = cert.to_der().expect("inter to_der");
        let priv_pkcs8_der = signing_key
            .to_pkcs8_der()
            .expect("inter pkcs8")
            .as_bytes()
            .to_vec();
        BuiltCert {
            cert_der,
            priv_pkcs8_der,
        }
    }

    /// Build an S/MIME leaf cert signed by `parent` (KeyUsage digitalSignature
    /// | keyEncipherment, EKU emailProtection, SAN rfc822Name(email), SKI).
    /// Mirrors the production `cert::build_self_signed_smime_cert` leaf shape
    /// but signed by the parent CA (so the issuer chain is real).
    #[doc(hidden)]
    pub fn build_leaf_signed_by(email: &str, parent: &BuiltCert) -> BuiltCert {
        build_leaf_inner(email, parent, &[])
    }

    /// Build an S/MIME leaf cert signed by `parent` that ALSO carries a
    /// `cRLDistributionPoints` extension with one `fullName` URI per entry of
    /// `dp_urls` (SP4c-3b). The receive hook extracts these URLs and fetches
    /// the CRLs they point at. Same leaf shape as [`build_leaf_signed_by`].
    #[doc(hidden)]
    pub fn build_leaf_signed_by_with_dp(
        email: &str,
        parent: &BuiltCert,
        dp_urls: &[&str],
    ) -> BuiltCert {
        build_leaf_inner(email, parent, dp_urls)
    }

    fn build_leaf_inner(email: &str, parent: &BuiltCert, dp_urls: &[&str]) -> BuiltCert {
        let mut rng = rand::rng();
        let signing_key = SigningKey::generate_from_rng(&mut rng);
        let verifying_key = signing_key.verifying_key();
        let pub_spki = SubjectPublicKeyInfo::from_key(verifying_key).expect("leaf spki");
        let ski = SubjectKeyIdentifier::try_from(pub_spki.owned_to_ref()).expect("leaf ski");

        let cn = email
            .split('@')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("leaf");
        let subject = Name::from_str(&format!("CN={cn}")).expect("leaf subject");
        let parent_cert = <x509_cert::Certificate as der::Decode>::from_der(&parent.cert_der)
            .expect("parse parent");
        let issuer = parent_cert.tbs_certificate().subject().clone();
        let profile = TestProfile { subject, issuer };

        let secs = now_secs();
        let serial = SerialNumber::from(secs as u32);
        let validity =
            Validity::from_now(Duration::from_secs(TEST_VALIDITY_SECS)).expect("leaf val");

        let mut builder =
            CertificateBuilder::new(profile, serial, validity, pub_spki).expect("leaf builder");
        builder
            .add_extension(&KeyUsage(
                KeyUsages::DigitalSignature | KeyUsages::KeyEncipherment,
            ))
            .expect("leaf ku");
        builder
            .add_extension(&ExtendedKeyUsage(vec![EKU_EMAIL_PROTECTION]))
            .expect("leaf eku");
        let email_ia5 = der::asn1::Ia5String::new(email.as_bytes()).expect("leaf san");
        builder
            .add_extension(&SubjectAltName(vec![GeneralName::Rfc822Name(email_ia5)]))
            .expect("leaf san");
        builder.add_extension(&ski).expect("leaf ski");

        // The CRL distribution-point extension (SP4c-3b), when URLs are given.
        if !dp_urls.is_empty() {
            let names: Vec<GeneralName> = dp_urls
                .iter()
                .map(|u| {
                    GeneralName::UniformResourceIdentifier(
                        der::asn1::Ia5String::new(u.as_bytes()).expect("dp uri ia5"),
                    )
                })
                .collect();
            let dp = x509_cert::ext::pkix::crl::dp::DistributionPoint {
                distribution_point: Some(
                    x509_cert::ext::pkix::name::DistributionPointName::FullName(names),
                ),
                reasons: None,
                crl_issuer: None,
            };
            builder
                .add_extension(&x509_cert::ext::pkix::CrlDistributionPoints(vec![dp]))
                .expect("leaf dp");
        }

        let parent_sk = SigningKey::from_pkcs8_der(&parent.priv_pkcs8_der).expect("parent key");
        let cert = builder
            .build::<_, DerSignature>(&parent_sk)
            .expect("leaf build");
        let cert_der = cert.to_der().expect("leaf to_der");
        let priv_pkcs8_der = signing_key
            .to_pkcs8_der()
            .expect("leaf pkcs8")
            .as_bytes()
            .to_vec();
        BuiltCert {
            cert_der,
            priv_pkcs8_der,
        }
    }

    /// Build a CMS SignedData over `payload` signed by `signer` (cert + priv
    /// DER), embedding the signer cert PLUS every `extra_certs` entry in the
    /// `certificates` set. Mirrors the production `cms_build::build_signed_data`
    /// shape but accepts extra certs (the production builder only embeds the
    /// signer leaf). Use to assemble a SignedData whose chain validation needs
    /// intermediates NOT in the cert set (pass them via the backend's stored
    /// intermediates table instead).
    #[doc(hidden)]
    pub fn build_signed_data_with_certs(
        payload: &[u8],
        signer_cert_der: &[u8],
        signer_priv_pkcs8_der: &[u8],
        extra_certs: &[Vec<u8>],
    ) -> Vec<u8> {
        use cms::builder::{SignedDataBuilder, SignerInfoBuilder};
        use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
        use cms::signed_data::{EncapsulatedContentInfo, SignerIdentifier};
        use der::Any;
        use spki::AlgorithmIdentifierOwned;
        use x509_cert::Certificate;

        let cert = <Certificate as der::Decode>::from_der(signer_cert_der).expect("signer parse");
        let tbs = cert.tbs_certificate();
        let sid = SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: tbs.issuer().clone(),
            serial_number: tbs.serial_number().clone(),
        });
        let secret = p256::SecretKey::from_pkcs8_der(signer_priv_pkcs8_der).expect("signer key");
        let signing_key = p256::ecdsa::SigningKey::from(&secret);

        let econtent = Some(Any::new(der::Tag::OctetString, payload.to_vec()).expect("econtent"));
        let encap = EncapsulatedContentInfo {
            econtent_type: const_oid::db::rfc5911::ID_DATA,
            econtent,
        };
        let digest_algorithm = AlgorithmIdentifierOwned {
            // SHA-256 OID (2.16.840.1.101.3.4.2.1) - inlined because
            // `const_oid::db::rfc5911::ID_SHA_256` is not a real constant and
            // the existing `cms_build::ID_SHA_256` / `cms_parse::ID_SHA_256`
            // are private to their modules.
            oid: const_oid::ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1"),
            parameters: None,
        };

        let signer_info_builder = SignerInfoBuilder::new(
            sid,
            digest_algorithm.clone(),
            &encap,
            None, // encapsulated → builder computes the digest
        )
        .expect("signer info");

        let mut builder = SignedDataBuilder::new(&encap);
        builder
            .add_digest_algorithm(digest_algorithm)
            .expect("add digest alg")
            .add_certificate(CertificateChoices::Certificate(cert))
            .expect("add signer cert");
        for extra_der in extra_certs {
            let extra_cert =
                <Certificate as der::Decode>::from_der(extra_der).expect("extra cert parse");
            builder
                .add_certificate(CertificateChoices::Certificate(extra_cert))
                .expect("add extra cert");
        }
        let content_info = builder
            .add_signer_info::<ecdsa::SigningKey<p256::NistP256>, p256::ecdsa::DerSignature>(
                signer_info_builder,
                &signing_key,
            )
            .expect("add signer info")
            .build()
            .expect("build SignedData");
        content_info.to_der().expect("ContentInfo to_der")
    }

    /// Build a CRL signed by `issuer` (SP4c-3b). The CRL revokes the serial of
    /// every cert in `revoked`; an empty slice yields a CRL with no revoked
    /// entries (a covering cert reads "good"). The validity window is now to
    /// now+7 days with a `nextUpdate`, so the chain's as-of-signing-time check
    /// reads it fresh. Mirrors the chain.rs spike CRL fixture; the issuer must
    /// carry `cRLSign` (the CA builders here do).
    #[doc(hidden)]
    pub fn build_crl(issuer: &BuiltCert, revoked: &[&BuiltCert]) -> Vec<u8> {
        use x509_cert::builder::CrlBuilder;
        use x509_cert::crl::RevokedCert as CrlRevokedCert;
        use x509_cert::ext::pkix::crl::CrlNumber;

        let issuer_cert = <x509_cert::Certificate as der::Decode>::from_der(&issuer.cert_der)
            .expect("parse issuer for CRL");
        let issuer_sk =
            SigningKey::from_pkcs8_der(&issuer.priv_pkcs8_der).expect("issuer key for CRL");

        let crl_number = CrlNumber(der::asn1::Uint::new(&[1u8]).expect("crl number uint"));
        let validity: Validity =
            Validity::from_now(Duration::from_secs(7 * 24 * 60 * 60)).expect("crl validity");

        let mut builder =
            CrlBuilder::new_with_this_update(&issuer_cert, crl_number, validity.not_before)
                .expect("crl builder")
                .with_next_update(Some(validity.not_after));

        if !revoked.is_empty() {
            let entries: Vec<CrlRevokedCert> = revoked
                .iter()
                .map(|leaf| {
                    let cert = <x509_cert::Certificate as der::Decode>::from_der(&leaf.cert_der)
                        .expect("parse revoked cert");
                    CrlRevokedCert {
                        serial_number: cert.tbs_certificate().serial_number().clone(),
                        revocation_date: validity.not_before,
                        crl_entry_extensions: None,
                    }
                })
                .collect();
            builder = builder.with_certificates(entries.into_iter());
        }

        let crl = builder
            .build::<_, DerSignature>(&issuer_sk)
            .expect("crl build/sign");
        crl.to_der().expect("crl to_der")
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1)
    }
}

#[cfg(feature = "testing")]
#[doc(hidden)]
pub use inner::{
    build_crl, build_intermediate_signed_by, build_leaf_signed_by, build_leaf_signed_by_with_dp,
    build_self_signed_ca, build_signed_data_with_certs, BuiltCert,
};
