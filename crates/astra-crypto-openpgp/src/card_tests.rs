// SPDX-License-Identifier: GPL-3.0-only
//! W6 OpenPGP-card routing tests (no hardware): the astra-crypto-core
//! `MockTokenProvider` emulates the card (presence gating, PIN sessions,
//! typed errors) while the actual "on-card" crypto is injected as closures
//! over the SAME keys Sequoia generated - so the real card-routed paths
//! (`CardSigner` / `CardDecryptor` shims, `OpenpgpBackend` token dispatch,
//! `engine::sign_detached_with_signer` / `decrypt_with_token` /
//! `encrypt_with_signer`) run end to end.
//!
//! Covered algorithms:
//! - **RSA 3k** card keys (sign + verify, encrypt + decrypt) - the classic
//!   smartcard shape.
//! - **Ed25519 / Cv25519 (ECDH)** card keys - GnuPG's modern default card
//!   shape and this engine's generated-key shape.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use astra_crypto_core::mock_token::MockTokenProvider;
    use astra_crypto_core::{
        CryptoBackend, CryptoError, CryptoPolicy, DecryptOp, EncryptOp, Fingerprint, KeyHandle,
        KeyHandleRef, KeyId, KeyStore, KeyUsage, Part, PartId, PartKind, SecretBox,
        SerializationStrategy, SignOp, SignatureState, Standard, StoredKey, TokenKeyId,
        TokenProvider, TokenSignMechanism, VerifyOp,
    };
    use async_trait::async_trait;
    use openpgp::cert::prelude::*;
    use openpgp::cert::CipherSuite;
    use openpgp::crypto::mpi;
    use sequoia_openpgp as openpgp;

    use crate::{keymap, OpenpgpBackend};

    // ─────────────────────────── fixtures ───────────────────────────

    /// In-memory `KeyStore` (same shape as the smime token tests'): token
    /// rows carry the PUBLIC cert blob with `private_data: None`.
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

    const CARD_SERIAL: &str = "MOCK-CARD-1";
    const CARD_PIN: &str = "123456";
    const CARD_KEY_ID: &str = "sig";

    fn token_handle(fp: &str) -> KeyHandleRef {
        KeyHandleRef {
            handle: KeyHandle::Token {
                token_serial: CARD_SERIAL.into(),
                key_id: TokenKeyId(CARD_KEY_ID.into()),
            },
            standard: Standard::OpenPgp,
            fingerprint: Fingerprint::new(fp),
            usage: KeyUsage::SignAndEncrypt,
            algorithm: "card".into(),
        }
    }

    fn software_handle(fp: &str) -> KeyHandleRef {
        KeyHandleRef {
            handle: KeyHandle::Software(KeyId(format!("openpgp|{fp}"))),
            standard: Standard::OpenPgp,
            fingerprint: Fingerprint::new(fp),
            usage: KeyUsage::SignAndEncrypt,
            algorithm: "default".into(),
        }
    }

    fn gen_cert(suite: CipherSuite, uid: &str) -> openpgp::Cert {
        let (cert, _rev) = CertBuilder::new()
            .set_cipher_suite(suite)
            .add_userid(uid)
            .add_transport_encryption_subkey()
            .add_signing_subkey()
            .generate()
            .expect("cert generation");
        cert
    }

    /// Register a token key row (public-only) + the recipient software row
    /// for the same cert.
    fn register(store: &MemStore, cert: &openpgp::Cert) -> (KeyHandleRef, KeyHandleRef) {
        let fp = cert.fingerprint().to_hex().to_ascii_lowercase();
        let public = keymap::cert_to_public_blob(cert).expect("public blob");
        let t = token_handle(&fp);
        store.insert(StoredKey {
            handle: t.clone(),
            public_data: public.clone(),
            private_data: None,
        });
        let s = software_handle(&fp);
        store.insert(StoredKey {
            handle: s.clone(),
            public_data: public,
            private_data: None,
        });
        (t, s)
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
            CARD_SERIAL,
            SecretBox::new(Box::new(CARD_PIN.to_string())),
            true,
        )
        .expect("unlock with correct PIN");
    }

    /// Extract the cert's signing subkey as a software KeyPair (the mock
    /// "card" uses it to emulate the on-card private op).
    fn signing_keypair(cert: &openpgp::Cert) -> openpgp::crypto::KeyPair {
        use openpgp::policy::StandardPolicy;
        let p = StandardPolicy::new();
        cert.keys()
            .unencrypted_secret()
            .with_policy(&p, None)
            .for_signing()
            .next()
            .expect("signing subkey")
            .key()
            .clone()
            .into_keypair()
            .expect("keypair")
    }

    /// Extract the cert's transport-encryption subkey (with secret).
    fn encryption_keypair(cert: &openpgp::Cert) -> openpgp::crypto::KeyPair {
        use openpgp::policy::StandardPolicy;
        let p = StandardPolicy::new();
        cert.keys()
            .unencrypted_secret()
            .with_policy(&p, None)
            .for_transport_encryption()
            .next()
            .expect("encryption subkey")
            .key()
            .clone()
            .into_keypair()
            .expect("keypair")
    }

    /// mpi::Signature → the raw wire form the card would return (mirrors the
    /// CardSigner reassembly contract).
    fn signature_to_raw(public: &mpi::PublicKey, sig: &mpi::Signature) -> Vec<u8> {
        match (public, sig) {
            (mpi::PublicKey::RSA { .. }, mpi::Signature::RSA { s }) => s.value().to_vec(),
            (mpi::PublicKey::EdDSA { .. }, mpi::Signature::EdDSA { r, s }) => {
                let mut raw = Vec::with_capacity(64);
                // R may carry the 0x40 point prefix (33 bytes) - strip to the
                // raw 32-byte form the card returns.
                let r_bytes = r.value();
                let r32 = if r_bytes.len() == 33 && r_bytes[0] == 0x40 {
                    &r_bytes[1..]
                } else {
                    r_bytes
                };
                raw.extend_from_slice(r32);
                raw.extend_from_slice(&s.value_padded(32).expect("s fits 32 bytes"));
                raw
            }
            other => panic!("unsupported fixture combination: {other:?}"),
        }
    }

    // ───────────────────────── RSA card ─────────────────────────

    #[tokio::test]
    async fn card_rsa_sign_then_verify_round_trips() {
        let cert = gen_cert(CipherSuite::RSA3k, "card-rsa@example.org");
        let pair = Mutex::new(signing_keypair(&cert));
        let public_mpis = pair.lock().unwrap().public().mpis().clone();

        let store = Arc::new(MemStore::new());
        let (token_ref, _soft) = register(&store, &cert);

        let mock = MockTokenProvider::openpgp_card(CARD_SERIAL, CARD_PIN);
        mock.add_key_id(CARD_KEY_ID, "RSA-3072", KeyUsage::Sign);
        // Emulated on-card op: PSO:COMPUTE DIGITAL SIGNATURE (RSA DigestInfo).
        mock.on_sign(move |_key, mech, digest| {
            assert_eq!(mech, TokenSignMechanism::RsaPkcs1Sha256);
            let sig = openpgp::crypto::Signer::sign(
                &mut *pair.lock().unwrap(),
                openpgp::types::HashAlgorithm::SHA256,
                digest,
            )
            .map_err(|e| CryptoError::Token(format!("on-card RSA sign: {e}")))?;
            Ok(signature_to_raw(&public_mpis, &sig))
        });
        unlock(&mock);

        let backend = OpenpgpBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let payload = b"signed by the OpenPGP card";
        let signed = backend
            .sign(SignOp {
                payload,
                signing_key: token_ref,
                detached: true,
            })
            .await
            .expect("card sign");
        let result = backend
            .verify(VerifyOp { signed: &signed })
            .await
            .expect("verify");
        assert!(
            matches!(result.state, SignatureState::ValidVerified),
            "card signature must verify against the public cert, got {:?} ({:?})",
            result.state,
            result.failure_reason
        );
    }

    #[tokio::test]
    async fn card_rsa_encrypt_then_decrypt_round_trips() {
        let cert = gen_cert(CipherSuite::RSA3k, "card-rsa-dec@example.org");
        let pair = Mutex::new(encryption_keypair(&cert));

        let store = Arc::new(MemStore::new());
        let (token_ref, soft_ref) = register(&store, &cert);

        let mock = MockTokenProvider::openpgp_card(CARD_SERIAL, CARD_PIN);
        mock.add_key_id(CARD_KEY_ID, "RSA-3072", KeyUsage::Encrypt);
        // Emulated on-card op: PSO:DECIPHER - RSA PKCS#1 unwrap. The closure
        // receives the raw RSA ciphertext MPI; emulate via the software
        // decryptor and return the unpadded block (algo || sk || checksum).
        mock.on_decrypt(move |_key, ciphertext| {
            let esk = mpi::Ciphertext::RSA {
                c: mpi::MPI::new(ciphertext),
            };
            // The software decryptor returns the FULL unpadded block as the
            // "session key" - the same shape the card returns (Sequoia's
            // PKESK layer parses algo||key||checksum out of it).
            let block = openpgp::crypto::Decryptor::decrypt(&mut *pair.lock().unwrap(), &esk, None)
                .map_err(|e| CryptoError::Token(format!("on-card RSA decrypt: {e}")))?;
            Ok(block.as_ref().to_vec())
        });
        unlock(&mock);

        let backend = OpenpgpBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let plaintext = b"encrypted to the OpenPGP card";
        let envelope = backend
            .encrypt(EncryptOp {
                parts: &[body_part(plaintext)],
                serialization: SerializationStrategy::SingleMimeBlob,
                recipients: &[soft_ref],
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
            .expect("card decrypt");
        assert_eq!(payload.parts[0].data, plaintext);
    }

    // ───────────────────────── Ed25519 / Cv25519 card ─────────────────────────

    #[tokio::test]
    async fn card_ed25519_sign_then_verify_round_trips() {
        let cert = gen_cert(CipherSuite::Cv25519, "card-ed@example.org");
        let pair = Mutex::new(signing_keypair(&cert));
        let public_mpis = pair.lock().unwrap().public().mpis().clone();

        let store = Arc::new(MemStore::new());
        let (token_ref, _soft) = register(&store, &cert);

        let mock = MockTokenProvider::openpgp_card(CARD_SERIAL, CARD_PIN);
        mock.add_key_id(CARD_KEY_ID, "Ed25519", KeyUsage::Sign);
        // Emulated on-card op: the card signs the OpenPGP hash output with
        // Ed25519 and returns R || S (64 bytes).
        mock.on_sign(move |_key, mech, digest| {
            assert_eq!(mech, TokenSignMechanism::Ed25519);
            let sig = openpgp::crypto::Signer::sign(
                &mut *pair.lock().unwrap(),
                openpgp::types::HashAlgorithm::SHA512,
                digest,
            )
            .map_err(|e| CryptoError::Token(format!("on-card Ed25519 sign: {e}")))?;
            Ok(signature_to_raw(&public_mpis, &sig))
        });
        unlock(&mock);

        let backend = OpenpgpBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let payload = b"ed25519 card payload";
        let signed = backend
            .sign(SignOp {
                payload,
                signing_key: token_ref,
                detached: true,
            })
            .await
            .expect("card sign");
        let result = backend
            .verify(VerifyOp { signed: &signed })
            .await
            .expect("verify");
        assert!(
            matches!(result.state, SignatureState::ValidVerified),
            "Ed25519 card signature must verify, got {:?} ({:?})",
            result.state,
            result.failure_reason
        );
    }

    #[tokio::test]
    async fn card_cv25519_encrypt_then_decrypt_round_trips() {
        let cert = gen_cert(CipherSuite::Cv25519, "card-cv@example.org");
        // Extract the ECDH scalar for the emulated on-card ECDH.
        let scalar: [u8; 32] = {
            use openpgp::policy::StandardPolicy;
            let p = StandardPolicy::new();
            let key = cert
                .keys()
                .unencrypted_secret()
                .with_policy(&p, None)
                .for_transport_encryption()
                .next()
                .expect("encryption subkey")
                .key()
                .clone();
            let secret = key.parts_into_secret().expect("secret key");
            match secret.secret() {
                // `map` hands us `&mpi::SecretKeyMaterial`; extract the
                // scalar into an OWNED array inside the closure.
                openpgp::packet::key::SecretKeyMaterial::Unencrypted(m) => m.map(|mm| match mm {
                    mpi::SecretKeyMaterial::ECDH { scalar } => {
                        // `value_padded` returns a zeroizing Protected (32
                        // bytes); OpenPGP stores the Cv25519 scalar
                        // big-endian, x25519 wants little-endian (see
                        // sequoia's own software decrypt path).
                        let mut r = scalar.value_padded(32);
                        r.reverse();
                        <[u8; 32]>::try_from(AsRef::<[u8]>::as_ref(&r)).expect("32-byte scalar")
                    }
                    other => panic!("expected ECDH secret, got {other:?}"),
                }),
                other => panic!("expected unencrypted secret, got {other:?}"),
            }
        };

        let store = Arc::new(MemStore::new());
        let (token_ref, soft_ref) = register(&store, &cert);

        let mock = MockTokenProvider::openpgp_card(CARD_SERIAL, CARD_PIN);
        mock.add_key_id(CARD_KEY_ID, "Cv25519", KeyUsage::Encrypt);
        // Emulated on-card op: PSO:DECIPHER (ECDH) - X25519 scalar multiply
        // against the ephemeral public point, return the shared point S.
        mock.on_decrypt(move |_key, ephemeral_point| {
            let e: [u8; 32] = ephemeral_point.try_into().map_err(|_| {
                CryptoError::Token(format!(
                    "ephemeral point must be 32 bytes, got {}",
                    ephemeral_point.len()
                ))
            })?;
            let secret = x25519_dalek::StaticSecret::from(scalar);
            let public = x25519_dalek::PublicKey::from(e);
            Ok(secret.diffie_hellman(&public).to_bytes().to_vec())
        });
        unlock(&mock);

        let backend = OpenpgpBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let plaintext = b"cv25519 card plaintext";
        let envelope = backend
            .encrypt(EncryptOp {
                parts: &[body_part(plaintext)],
                serialization: SerializationStrategy::SingleMimeBlob,
                recipients: &[soft_ref],
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
            .expect("card decrypt");
        assert_eq!(payload.parts[0].data, plaintext);
    }

    #[tokio::test]
    async fn card_sign_then_encrypt_and_decrypt_inline_signature() {
        // Full combined flow: encrypt WITH a card inline-signature, then
        // decrypt with the SAME card (signer == decryptor, the round-trip
        // shape the engine's embedded-signature check supports).
        let cert = gen_cert(CipherSuite::RSA3k, "card-full@example.org");
        let sign_pair = Mutex::new(signing_keypair(&cert));
        let sign_mpis = sign_pair.lock().unwrap().public().mpis().clone();
        let dec_pair = Mutex::new(encryption_keypair(&cert));

        let store = Arc::new(MemStore::new());
        let (token_ref, soft_ref) = register(&store, &cert);

        let mock = MockTokenProvider::openpgp_card(CARD_SERIAL, CARD_PIN);
        mock.on_sign(move |_key, _mech, digest| {
            let sig = openpgp::crypto::Signer::sign(
                &mut *sign_pair.lock().unwrap(),
                openpgp::types::HashAlgorithm::SHA256,
                digest,
            )
            .map_err(|e| CryptoError::Token(format!("on-card sign: {e}")))?;
            Ok(signature_to_raw(&sign_mpis, &sig))
        });
        mock.on_decrypt(move |_key, ciphertext| {
            let esk = mpi::Ciphertext::RSA {
                c: mpi::MPI::new(ciphertext),
            };
            let block =
                openpgp::crypto::Decryptor::decrypt(&mut *dec_pair.lock().unwrap(), &esk, None)
                    .map_err(|e| CryptoError::Token(format!("on-card decrypt: {e}")))?;
            Ok(block.as_ref().to_vec())
        });
        unlock(&mock);

        let backend = OpenpgpBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let plaintext = b"card-signed and card-decrypted";
        let envelope = backend
            .encrypt(EncryptOp {
                parts: &[body_part(plaintext)],
                serialization: SerializationStrategy::SingleMimeBlob,
                recipients: &[soft_ref],
                sign_with: Some(token_ref.clone()),
            })
            .await
            .expect("card sign-then-encrypt");
        let payload = backend
            .decrypt(DecryptOp {
                envelope: &envelope,
                decryption_key: token_ref,
            })
            .await
            .expect("card decrypt (embedded signature verified in-engine)");
        assert_eq!(payload.parts[0].data, plaintext);
    }

    // ───────────────────────── typed errors ─────────────────────────

    #[tokio::test]
    async fn card_sign_without_unlock_is_pin_required() {
        let cert = gen_cert(CipherSuite::Cv25519, "card-pin@example.org");
        let store = Arc::new(MemStore::new());
        let (token_ref, _soft) = register(&store, &cert);

        let mock = MockTokenProvider::openpgp_card(CARD_SERIAL, CARD_PIN);
        mock.on_sign(|_k, _m, _d| Ok(vec![0u8; 64]));
        // NOTE: no unlock - the private op must demand the PIN, and the
        // typed error must survive the sequoia/engine boundary.
        let backend = OpenpgpBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));

        let err = backend
            .sign(SignOp {
                payload: b"x",
                signing_key: token_ref,
                detached: true,
            })
            .await
            .expect_err("must fail without PIN");
        assert!(
            matches!(err, CryptoError::TokenPinRequired(_)),
            "expected TokenPinRequired, got {err:?}"
        );
    }

    #[tokio::test]
    async fn card_decrypt_with_removed_card_is_not_present() {
        let cert = gen_cert(CipherSuite::Cv25519, "card-gone@example.org");
        let store = Arc::new(MemStore::new());
        let (token_ref, soft_ref) = register(&store, &cert);

        let mock = MockTokenProvider::openpgp_card(CARD_SERIAL, CARD_PIN);
        mock.on_decrypt(|_k, _c| Ok(vec![0u8; 32]));

        let backend_unlocked =
            OpenpgpBackend::new(Arc::new(MemStore::new()), CryptoPolicy::default_baseline());
        // Build the envelope with a separate software-only backend (no token
        // involvement on the encrypt side).
        let store2 = Arc::new(MemStore::new());
        let _ = backend_unlocked; // (clarity: encrypt uses store2's backend below)
        store2.insert(StoredKey {
            handle: soft_ref.clone(),
            public_data: keymap::cert_to_public_blob(&cert).expect("public blob"),
            private_data: None,
        });
        let enc_backend = OpenpgpBackend::new(store2, CryptoPolicy::default_baseline());
        let envelope = enc_backend
            .encrypt(EncryptOp {
                parts: &[body_part(b"x")],
                serialization: SerializationStrategy::SingleMimeBlob,
                recipients: &[soft_ref],
                sign_with: None,
            })
            .await
            .expect("encrypt");

        unlock(&mock);
        mock.remove_token(CARD_SERIAL);
        let backend = OpenpgpBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));
        let err = backend
            .decrypt(DecryptOp {
                envelope: &envelope,
                decryption_key: token_ref,
            })
            .await
            .expect_err("must fail with removed card");
        assert!(
            matches!(err, CryptoError::TokenNotPresent(_)),
            "expected TokenNotPresent, got {err:?}"
        );
    }

    #[tokio::test]
    async fn card_handle_without_provider_is_typed_not_present() {
        let cert = gen_cert(CipherSuite::Cv25519, "card-noprovider@example.org");
        let store = Arc::new(MemStore::new());
        let (token_ref, _soft) = register(&store, &cert);

        let backend = OpenpgpBackend::new(store, CryptoPolicy::default_baseline());
        let err = backend
            .sign(SignOp {
                payload: b"x",
                signing_key: token_ref,
                detached: true,
            })
            .await
            .expect_err("must fail without a provider");
        assert!(
            matches!(err, CryptoError::TokenNotPresent(_)),
            "expected TokenNotPresent, got {err:?}"
        );
    }

    #[tokio::test]
    async fn decrypt_with_wrong_card_stays_typed_no_matching_key() {
        // Regression guard: a token decrypt against a message addressed to a
        // DIFFERENT key must surface `NoMatchingKey` (the receive loop's
        // try-next-key signal), not a spurious token error.
        let ours = gen_cert(CipherSuite::Cv25519, "card-ours@example.org");
        let theirs = gen_cert(CipherSuite::Cv25519, "card-theirs@example.org");

        let store = Arc::new(MemStore::new());
        let (token_ref, _soft) = register(&store, &ours);

        // Encrypt to THEIR key (software-only backend).
        let store2 = Arc::new(MemStore::new());
        let their_fp = theirs.fingerprint().to_hex().to_ascii_lowercase();
        let their_ref = software_handle(&their_fp);
        store2.insert(StoredKey {
            handle: their_ref.clone(),
            public_data: keymap::cert_to_public_blob(&theirs).expect("public blob"),
            private_data: None,
        });
        let enc_backend = OpenpgpBackend::new(store2, CryptoPolicy::default_baseline());
        let envelope = enc_backend
            .encrypt(EncryptOp {
                parts: &[body_part(b"not for our card")],
                serialization: SerializationStrategy::SingleMimeBlob,
                recipients: &[their_ref],
                sign_with: None,
            })
            .await
            .expect("encrypt");

        let mock = MockTokenProvider::openpgp_card(CARD_SERIAL, CARD_PIN);
        // The emulated card faithfully ECDH-derives against ANY ephemeral
        // point (as a real card would); the unwrap then fails because the
        // slot was addressed to a different key.
        mock.on_decrypt(|_k, point| Ok(vec![0x42u8; point.len().max(32)]));
        unlock(&mock);

        let backend = OpenpgpBackend::new(store, CryptoPolicy::default_baseline())
            .with_token_provider(Arc::new(mock));
        let err = backend
            .decrypt(DecryptOp {
                envelope: &envelope,
                decryption_key: token_ref,
            })
            .await
            .expect_err("wrong card must fail");
        assert!(
            matches!(err, CryptoError::NoMatchingKey(_)),
            "expected NoMatchingKey (wrong-key signal), got {err:?}"
        );
    }
}
