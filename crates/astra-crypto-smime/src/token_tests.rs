// SPDX-License-Identifier: GPL-3.0-only
//! W6 hardware-token routing tests (no hardware): the crypto-core
//! `MockTokenProvider` emulates the device (presence gating, PIN sessions,
//! typed errors) while the actual "on-token" crypto is injected as closures
//! over the same `rsa`/`p256` stacks the software paths use. This exercises
//! the REAL token-routed code - `SmimeBackend::sign` / `encrypt(sign_with)` /
//! `decrypt` with `KeyHandle::Token`, the cms callback signers, and the token
//! CEK-unwrap delegation - end to end.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use astra_crypto_core::mock_token::MockTokenProvider;
    use astra_crypto_core::{
        CryptoBackend, CryptoError, CryptoPolicy, DecryptOp, EncryptOp, Fingerprint, KeyHandle,
        KeyHandleRef, KeyId, KeyStore, KeyUsage, Part, PartId, PartKind, SecretBox,
        SerializationStrategy, SignOp, SignatureState, Standard, StoredKey, TokenKeyId,
        TokenKeyRef, TokenProvider, TokenSignMechanism, VerifyOp,
    };
    use async_trait::async_trait;

    use crate::SmimeBackend;

    // ─────────────────────────── fixtures ───────────────────────────

    /// In-memory `KeyStore` that stores by handle (Software AND Token) -
    /// token rows carry `public_data` (the cert) with `private_data: None`,
    /// exactly like the registration-time row the SqliteKeyStore serves.
    struct MemStore {
        keys: Mutex<HashMap<KeyHandle, StoredKey>>,
    }

    impl MemStore {
        fn new() -> Self {
            Self {
                keys: Mutex::new(HashMap::new()),
            }
        }

        fn insert(&self, key: StoredKey) {
            self.keys
                .lock()
                .unwrap()
                .insert(key.handle.handle.clone(), key);
        }
    }

    #[async_trait]
    impl KeyStore for MemStore {
        async fn put(&self, key: StoredKey) -> astra_crypto_core::Result<KeyHandleRef> {
            let h = key.handle.clone();
            self.insert(key);
            Ok(h)
        }
        async fn get(&self, handle: &KeyHandle) -> astra_crypto_core::Result<Option<StoredKey>> {
            // `StoredKey` is not Clone (SecretBox hygiene) - rebuild the entry
            // with freshly-boxed private bytes.
            let found = self.keys.lock().unwrap().get(handle).map(|k| StoredKey {
                handle: k.handle.clone(),
                public_data: k.public_data.clone(),
                private_data: k.private_data.as_ref().map(|p| {
                    SecretBox::new(Box::new(
                        astra_crypto_core::secret::expose_bytes(p).to_vec(),
                    ))
                }),
            });
            Ok(found)
        }
        async fn find_by_email(
            &self,
            _standard: Standard,
            _email: &str,
        ) -> astra_crypto_core::Result<Vec<KeyHandleRef>> {
            Ok(vec![])
        }
        async fn remove(&self, handle: &KeyHandle) -> astra_crypto_core::Result<()> {
            self.keys.lock().unwrap().remove(handle);
            Ok(())
        }
    }

    /// Build an RSA self-signed cert + PKCS#8 key (same builder shape as the
    /// cms_parse round-trip fixtures).
    fn rsa_cert_and_key(id: u32) -> (Vec<u8>, Vec<u8>) {
        use der::Encode;
        use ecdsa::signature::Keypair;
        use pkcs8::EncodePrivateKey;
        use rsa::pkcs1v15::SigningKey;
        use sha2::Sha256;
        use std::str::FromStr;
        use std::time::Duration;
        use x509_cert::builder::profile::BuilderProfile;
        use x509_cert::builder::{Builder, CertificateBuilder};
        use x509_cert::certificate::TbsCertificate;
        use x509_cert::ext::pkix::{KeyUsage, KeyUsages};
        use x509_cert::name::Name;
        use x509_cert::serial_number::SerialNumber;
        use x509_cert::time::Validity;
        use x509_cert::SubjectPublicKeyInfo;

        struct RsaTestProfile {
            subject: Name,
        }
        impl BuilderProfile for RsaTestProfile {
            fn get_subject(&self) -> Name {
                self.subject.clone()
            }
            fn get_issuer(&self, subject: &Name) -> Name {
                subject.clone()
            }
            fn build_extensions(
                &self,
                _spk: spki::SubjectPublicKeyInfoRef<'_>,
                _issuer_spk: spki::SubjectPublicKeyInfoRef<'_>,
                _tbs: &TbsCertificate,
            ) -> x509_cert::builder::Result<Vec<x509_cert::ext::Extension>> {
                Ok(Vec::new())
            }
        }

        let mut rng = rand::rng();
        let rsa_priv = rsa::RsaPrivateKey::new(&mut rng, 3072).expect("generate RSA key");
        let signing_key = SigningKey::<Sha256>::new(rsa_priv.clone());
        let pub_spki =
            SubjectPublicKeyInfo::from_key(&signing_key.verifying_key()).expect("RSA SPKI");
        let subject = Name::from_str(&format!("CN=token-rsa-{id}")).expect("subject");
        let serial = SerialNumber::from(id);
        let validity =
            Validity::from_now(Duration::from_secs(365 * 24 * 60 * 60)).expect("validity");
        let mut builder =
            CertificateBuilder::new(RsaTestProfile { subject }, serial, validity, pub_spki)
                .expect("cert builder");
        builder
            .add_extension(&KeyUsage(
                KeyUsages::DigitalSignature | KeyUsages::KeyEncipherment,
            ))
            .expect("key usage");
        let cert = builder
            .build::<_, rsa::pkcs1v15::Signature>(&signing_key)
            .expect("build RSA cert");
        let priv_doc = rsa_priv.to_pkcs8_der().expect("RSA PKCS#8");
        (
            cert.to_der().expect("cert DER"),
            priv_doc.as_bytes().to_vec(),
        )
    }

    /// Build an ECDSA P-256 self-signed cert + PKCS#8 key (production builder).
    fn p256_cert_and_key(id: u32) -> (Vec<u8>, Vec<u8>) {
        let built =
            crate::cert::build_self_signed_smime_cert(&format!("token-ecc-{id}@kylins.com"))
                .expect("build P-256 cert");
        (built.cert_der, built.priv_pkcs8_der)
    }

    const TOKEN_SERIAL: &str = "MOCK-TOKEN-1";
    const TOKEN_PIN: &str = "123456";
    const TOKEN_KEY_ID: &str = "01";

    fn token_handle(fp: &str) -> KeyHandleRef {
        KeyHandleRef {
            handle: KeyHandle::Token {
                token_serial: TOKEN_SERIAL.into(),
                key_id: TokenKeyId(TOKEN_KEY_ID.into()),
            },
            standard: Standard::Smime,
            fingerprint: Fingerprint::new(fp),
            usage: KeyUsage::SignAndEncrypt,
            algorithm: "token".into(),
        }
    }

    fn recipient_handle(fp: &str) -> KeyHandleRef {
        KeyHandleRef {
            handle: KeyHandle::Software(KeyId(format!("smime|{fp}"))),
            standard: Standard::Smime,
            fingerprint: Fingerprint::new(fp),
            usage: KeyUsage::SignAndEncrypt,
            algorithm: "ECDSA-P256".into(),
        }
    }

    /// Register a token key (public-only row) in the store.
    fn register_token_key(store: &MemStore, fp: &str, cert_der: &[u8]) -> KeyHandleRef {
        let href = token_handle(fp);
        store.insert(StoredKey {
            handle: href.clone(),
            public_data: cert_der.to_vec(),
            private_data: None,
        });
        href
    }

    fn body_part(data: &[u8]) -> Part {
        Part {
            id: PartId("body".into()),
            kind: PartKind::Body,
            data: data.to_vec(),
        }
    }

    fn unlock(mock: &MockTokenProvider) {
        mock.unlock(
            TOKEN_SERIAL,
            SecretBox::new(Box::new(TOKEN_PIN.to_string())),
            true,
        )
        .expect("unlock with correct PIN");
    }

    // ───────────────────────── sign + verify ─────────────────────────

    #[tokio::test]
    async fn token_sign_rsa_then_verify_round_trips() {
        let (cert_der, priv_pkcs8) = rsa_cert_and_key(11);
        let rsa_priv = <rsa::RsaPrivateKey as pkcs8::DecodePrivateKey>::from_pkcs8_der(&priv_pkcs8)
            .expect("parse RSA key");
        let signing_key = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(rsa_priv);

        let store = Arc::new(MemStore::new());
        let signer_ref = register_token_key(&store, "fp-token-rsa", &cert_der);

        let mock = MockTokenProvider::pkcs11(TOKEN_SERIAL, TOKEN_PIN);
        mock.add_key_id(TOKEN_KEY_ID, "RSA-3072", KeyUsage::SignAndEncrypt);
        // Emulated on-token op: DigestInfo(SHA-256) || digest → raw RSA sign
        // (pkcs1v15 prehash signing - exactly the RsaPkcs1Sha256 contract).
        mock.on_sign(move |_key, mech, digest| {
            assert_eq!(mech, TokenSignMechanism::RsaPkcs1Sha256);
            use rsa::signature::hazmat::PrehashSigner;
            use rsa::signature::SignatureEncoding;
            let sig = signing_key.sign_prehash(digest).expect("on-token RSA sign");
            Ok(sig.to_vec())
        });
        unlock(&mock);

        let backend = SmimeBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let payload = b"signed by the token";
        let signed = backend
            .sign(SignOp {
                payload,
                signing_key: signer_ref,
                detached: false,
            })
            .await
            .expect("token sign");

        let result = backend
            .verify(VerifyOp { signed: &signed })
            .await
            .expect("verify");
        assert!(
            matches!(result.state, SignatureState::ValidUnverified),
            "token-produced SignedData must verify, got {:?}",
            result.state
        );
    }

    #[tokio::test]
    async fn token_sign_ecdsa_p256_then_verify_round_trips() {
        let (cert_der, priv_pkcs8) = p256_cert_and_key(12);
        let signing_key =
            <p256::ecdsa::SigningKey as pkcs8::DecodePrivateKey>::from_pkcs8_der(&priv_pkcs8)
                .expect("parse P-256 key");

        let store = Arc::new(MemStore::new());
        let signer_ref = register_token_key(&store, "fp-token-ecc", &cert_der);

        let mock = MockTokenProvider::pkcs11(TOKEN_SERIAL, TOKEN_PIN);
        mock.add_key_id(TOKEN_KEY_ID, "ECDSA-P256", KeyUsage::SignAndEncrypt);
        // Emulated on-token op: CKM_ECDSA over the digest → raw r || s.
        mock.on_sign(move |_key, mech, digest| {
            assert_eq!(mech, TokenSignMechanism::EcdsaP256Sha256);
            use p256::ecdsa::signature::hazmat::PrehashSigner;
            let sig: p256::ecdsa::Signature = signing_key
                .sign_prehash(digest)
                .expect("on-token ECDSA sign");
            Ok(sig.to_bytes().to_vec())
        });
        unlock(&mock);

        let backend = SmimeBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let payload = b"ecdsa token payload";
        let signed = backend
            .sign(SignOp {
                payload,
                signing_key: signer_ref,
                detached: false,
            })
            .await
            .expect("token sign");
        let result = backend
            .verify(VerifyOp { signed: &signed })
            .await
            .expect("verify");
        assert!(
            matches!(result.state, SignatureState::ValidUnverified),
            "ECDSA token signature must verify, got {:?}",
            result.state
        );
    }

    #[tokio::test]
    async fn token_sign_detached_covers_external_payload() {
        let (cert_der, priv_pkcs8) = p256_cert_and_key(13);
        let signing_key =
            <p256::ecdsa::SigningKey as pkcs8::DecodePrivateKey>::from_pkcs8_der(&priv_pkcs8)
                .expect("parse P-256 key");

        let store = Arc::new(MemStore::new());
        let signer_ref = register_token_key(&store, "fp-token-det", &cert_der);

        let mock = MockTokenProvider::pkcs11(TOKEN_SERIAL, TOKEN_PIN);
        mock.on_sign(move |_key, _mech, digest| {
            use p256::ecdsa::signature::hazmat::PrehashSigner;
            let sig: p256::ecdsa::Signature = signing_key.sign_prehash(digest).expect("sign");
            Ok(sig.to_bytes().to_vec())
        });
        unlock(&mock);

        let backend = SmimeBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let payload = b"detached token content";
        let signed = backend
            .sign(SignOp {
                payload,
                signing_key: signer_ref,
                detached: true,
            })
            .await
            .expect("detached token sign");
        let result = backend
            .verify(VerifyOp { signed: &signed })
            .await
            .expect("verify");
        assert!(matches!(result.state, SignatureState::ValidUnverified));
    }

    // ───────────────────────── encrypt + decrypt ─────────────────────────

    #[tokio::test]
    async fn token_decrypt_rsa_ktri_round_trips() {
        let (cert_der, priv_pkcs8) = rsa_cert_and_key(14);
        let rsa_priv = <rsa::RsaPrivateKey as pkcs8::DecodePrivateKey>::from_pkcs8_der(&priv_pkcs8)
            .expect("parse RSA key");

        let store = Arc::new(MemStore::new());
        let token_ref = register_token_key(&store, "fp-token-rsa-dec", &cert_der);
        // Recipient row for encrypt (public-only software handle).
        let rcpt_ref = recipient_handle("fp-token-rsa-dec");
        store.insert(StoredKey {
            handle: rcpt_ref.clone(),
            public_data: cert_der.clone(),
            private_data: None,
        });

        let mock = MockTokenProvider::pkcs11(TOKEN_SERIAL, TOKEN_PIN);
        mock.on_decrypt(move |_key, ciphertext| {
            rsa_priv
                .decrypt(rsa::Pkcs1v15Encrypt, ciphertext)
                .map_err(|e| CryptoError::Token(format!("on-token RSA decrypt: {e}")))
        });
        unlock(&mock);

        let backend = SmimeBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let plaintext = b"enveloped to the token";
        let envelope = backend
            .encrypt(EncryptOp {
                parts: &[body_part(plaintext)],
                serialization: SerializationStrategy::SingleMimeBlob,
                recipients: &[rcpt_ref],
                sign_with: None,
            })
            .await
            .expect("encrypt");

        let payload = backend
            .decrypt(DecryptOp {
                envelope: &envelope,
                decryption_key: token_ref,
            })
            .await
            .expect("token decrypt");
        assert_eq!(payload.parts[0].data, plaintext);
    }

    #[tokio::test]
    async fn token_decrypt_ecc_kari_round_trips() {
        let (cert_der, priv_pkcs8) = p256_cert_and_key(15);
        let secret = <p256::SecretKey as pkcs8::DecodePrivateKey>::from_pkcs8_der(&priv_pkcs8)
            .expect("parse P-256 key");

        let store = Arc::new(MemStore::new());
        let token_ref = register_token_key(&store, "fp-token-ecc-dec", &cert_der);
        let rcpt_ref = recipient_handle("fp-token-ecc-dec");
        store.insert(StoredKey {
            handle: rcpt_ref.clone(),
            public_data: cert_der.clone(),
            private_data: None,
        });

        let mock = MockTokenProvider::pkcs11(TOKEN_SERIAL, TOKEN_PIN);
        // Emulated on-token op: CKM_ECDH1_DERIVE against the ephemeral point.
        mock.on_decrypt(move |_key, ephemeral_sec1| {
            let point = p256::PublicKey::from_sec1_bytes(ephemeral_sec1)
                .map_err(|e| CryptoError::Token(format!("parse ephemeral point: {e}")))?;
            let shared = p256::ecdh::diffie_hellman(secret.to_nonzero_scalar(), point.as_affine());
            Ok(shared.raw_secret_bytes().to_vec())
        });
        unlock(&mock);

        let backend = SmimeBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let plaintext = b"kari enveloped to the token";
        let envelope = backend
            .encrypt(EncryptOp {
                parts: &[body_part(plaintext)],
                serialization: SerializationStrategy::SingleMimeBlob,
                recipients: &[rcpt_ref],
                sign_with: None,
            })
            .await
            .expect("encrypt");
        let payload = backend
            .decrypt(DecryptOp {
                envelope: &envelope,
                decryption_key: token_ref,
            })
            .await
            .expect("token kari decrypt");
        assert_eq!(payload.parts[0].data, plaintext);
    }

    #[tokio::test]
    async fn token_sign_then_encrypt_and_token_decrypt_full_round_trip() {
        // Sign with a token ECDSA key, encrypt to a software RSA... simpler:
        // sign with token ECDSA key, encrypt to the SAME token key's cert,
        // decrypt with the token. Proves sign_with routing + kari decrypt in
        // one E2E pass.
        let (cert_der, priv_pkcs8) = p256_cert_and_key(16);
        let signing_key =
            <p256::ecdsa::SigningKey as pkcs8::DecodePrivateKey>::from_pkcs8_der(&priv_pkcs8)
                .expect("parse P-256 signing key");
        let secret = <p256::SecretKey as pkcs8::DecodePrivateKey>::from_pkcs8_der(&priv_pkcs8)
            .expect("parse P-256 secret");

        let store = Arc::new(MemStore::new());
        let token_ref = register_token_key(&store, "fp-token-full", &cert_der);
        let rcpt_ref = recipient_handle("fp-token-full");
        store.insert(StoredKey {
            handle: rcpt_ref.clone(),
            public_data: cert_der.clone(),
            private_data: None,
        });

        let mock = MockTokenProvider::pkcs11(TOKEN_SERIAL, TOKEN_PIN);
        mock.on_sign(move |_key, _mech, digest| {
            use p256::ecdsa::signature::hazmat::PrehashSigner;
            let sig: p256::ecdsa::Signature = signing_key.sign_prehash(digest).expect("sign");
            Ok(sig.to_bytes().to_vec())
        });
        mock.on_decrypt(move |_key, ephemeral_sec1| {
            let point = p256::PublicKey::from_sec1_bytes(ephemeral_sec1)
                .map_err(|e| CryptoError::Token(format!("parse ephemeral: {e}")))?;
            Ok(
                p256::ecdh::diffie_hellman(secret.to_nonzero_scalar(), point.as_affine())
                    .raw_secret_bytes()
                    .to_vec(),
            )
        });
        unlock(&mock);

        let backend = SmimeBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let plaintext = b"token sign + token decrypt";
        let envelope = backend
            .encrypt(EncryptOp {
                parts: &[body_part(plaintext)],
                serialization: SerializationStrategy::SingleMimeBlob,
                recipients: &[rcpt_ref],
                sign_with: Some(token_ref.clone()),
            })
            .await
            .expect("sign-then-encrypt");

        let payload = backend
            .decrypt(DecryptOp {
                envelope: &envelope,
                decryption_key: token_ref,
            })
            .await
            .expect("token decrypt");
        // The decrypted content is the opaque SignedData (sign-then-encrypt);
        // verify it back to ValidUnverified via the backend.
        let ci = <cms::content_info::ContentInfo as der::Decode>::from_der(&payload.parts[0].data)
            .expect("inner content info");
        assert_eq!(ci.content_type, const_oid::db::rfc5911::ID_SIGNED_DATA);
    }

    // ───────────────────────── typed errors ─────────────────────────

    #[tokio::test]
    async fn token_sign_without_unlock_is_pin_required() {
        let (cert_der, _priv) = p256_cert_and_key(17);
        let store = Arc::new(MemStore::new());
        let signer_ref = register_token_key(&store, "fp-token-pin", &cert_der);

        let mock = MockTokenProvider::pkcs11(TOKEN_SERIAL, TOKEN_PIN);
        mock.on_sign(|_k, _m, _d| Ok(vec![0u8; 64]));
        // NOTE: no unlock() - the private op must demand the PIN.
        let backend = SmimeBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let err = backend
            .sign(SignOp {
                payload: b"x",
                signing_key: signer_ref,
                detached: false,
            })
            .await
            .expect_err("must fail without PIN");
        assert!(
            matches!(err, CryptoError::TokenPinRequired(_)),
            "expected TokenPinRequired, got {err:?}"
        );
    }

    #[tokio::test]
    async fn token_sign_with_removed_token_is_not_present() {
        let (cert_der, _priv) = p256_cert_and_key(18);
        let store = Arc::new(MemStore::new());
        let signer_ref = register_token_key(&store, "fp-token-gone", &cert_der);

        let mock = MockTokenProvider::pkcs11(TOKEN_SERIAL, TOKEN_PIN);
        mock.on_sign(|_k, _m, _d| Ok(vec![0u8; 64]));
        unlock(&mock);
        mock.remove_token(TOKEN_SERIAL);

        let backend = SmimeBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let err = backend
            .sign(SignOp {
                payload: b"x",
                signing_key: signer_ref,
                detached: false,
            })
            .await
            .expect_err("must fail with removed token");
        assert!(
            matches!(err, CryptoError::TokenNotPresent(_)),
            "expected TokenNotPresent, got {err:?}"
        );
    }

    #[tokio::test]
    async fn token_handle_without_provider_is_typed_not_present() {
        let (cert_der, _priv) = p256_cert_and_key(19);
        let store = Arc::new(MemStore::new());
        let signer_ref = register_token_key(&store, "fp-token-noprovider", &cert_der);

        // No with_token_provider - the token handle must fail fast + typed.
        let backend = SmimeBackend::new(store, CryptoPolicy::default_baseline());
        let err = backend
            .sign(SignOp {
                payload: b"x",
                signing_key: signer_ref,
                detached: false,
            })
            .await
            .expect_err("must fail without a provider");
        assert!(
            matches!(err, CryptoError::TokenNotPresent(_)),
            "expected TokenNotPresent, got {err:?}"
        );
    }

    #[test]
    fn unlock_with_wrong_pin_is_typed_incorrect() {
        let mock = MockTokenProvider::pkcs11(TOKEN_SERIAL, TOKEN_PIN);
        let err = mock
            .unlock(
                TOKEN_SERIAL,
                SecretBox::new(Box::new("000000".to_string())),
                true,
            )
            .expect_err("wrong PIN must fail");
        assert!(matches!(err, CryptoError::TokenPinIncorrect(_)));
        // And the token stays locked: a private op still demands the PIN.
        let key = TokenKeyRef::new(TOKEN_SERIAL, TokenKeyId(TOKEN_KEY_ID.into()));
        let err = mock
            .sign(&key, TokenSignMechanism::EcdsaP256Sha256, b"digest")
            .expect_err("still locked");
        assert!(matches!(err, CryptoError::TokenPinRequired(_)));
    }

    #[tokio::test]
    async fn software_sign_still_rejects_non_p256_algorithm() {
        // Regression guard: the software path's algorithm gate must be
        // unaffected by the token routing.
        let built = crate::cert::build_self_signed_smime_cert("soft@kylins.com").unwrap();
        let store = Arc::new(MemStore::new());
        let mut href = recipient_handle("fp-soft");
        href.algorithm = "RSA-4096".into();
        store.insert(StoredKey {
            handle: href.clone(),
            public_data: built.cert_der,
            private_data: Some(SecretBox::new(Box::new(built.priv_pkcs8_der))),
        });
        let backend = SmimeBackend::new(store, CryptoPolicy::default_baseline());
        let err = backend
            .sign(SignOp {
                payload: b"x",
                signing_key: href,
                detached: false,
            })
            .await
            .expect_err("software RSA must stay NotImplemented");
        assert!(matches!(err, CryptoError::NotImplemented(_)));
    }
}
