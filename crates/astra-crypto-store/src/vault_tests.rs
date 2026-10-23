// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the vault: the AAD-bound wrap/unwrap contract.

use super::*;

fn install_test_key() {
    set_test_master_key([0x42u8; 32]);
}

#[test]
fn aad_roundtrip_succeeds() {
    install_test_key();
    let blob = encrypt_with_aad(b"private key blob", b"astra:acct-1:private_key:1")
        .expect("encrypt_with_aad");
    let bytes = hex::decode(&blob).unwrap();
    assert_eq!(
        bytes[0], VAULT_VERSION_V1,
        "v1 blob must start with the version byte"
    );
    let pt = decrypt_with_aad(&blob, b"astra:acct-1:private_key:1").expect("decrypt_with_aad");
    assert_eq!(pt, b"private key blob");
}

#[test]
fn aad_wrong_context_fails() {
    install_test_key();
    let blob = encrypt_with_aad(b"private key blob", b"astra:acct-1:private_key:1")
        .expect("encrypt_with_aad");
    // Wrong AAD (a different account) must fail AEAD verification.
    let err = decrypt_with_aad(&blob, b"astra:acct-2:private_key:1");
    assert!(
        matches!(err, Err(CryptoVaultError::Aead(_))),
        "wrong AAD must reject: {err:?}"
    );
}

#[test]
fn aad_tampered_version_fails() {
    install_test_key();
    let blob = encrypt_with_aad(b"x", b"ctx").unwrap();
    let mut bytes = hex::decode(&blob).unwrap();
    bytes[0] = 0x02; // unsupported version
    let tampered = hex::encode(&bytes);
    let err = decrypt_with_aad(&tampered, b"ctx");
    assert!(matches!(err, Err(CryptoVaultError::UnsupportedVersion(2))));
}

#[test]
fn master_key_is_secret_box_backed() {
    // Compile-time guarantee: the cached master key is a zeroizing secret,
    // not a raw [u8; 32]. If someone reverts it to a plain array this fails
    // to compile.
    #[allow(clippy::used_underscore_items)]
    fn accept_secret(_s: &secrecy::SecretBox<[u8; 32]>) {}
    install_test_key();
    let guard = acquire_key().expect("acquire key");
    accept_secret(guard.guard.as_ref().expect("key installed"));
}

#[test]
fn short_ciphertext_is_rejected() {
    install_test_key();
    let err = decrypt_with_aad(&hex::encode([VAULT_VERSION_V1, 0x01, 0x02]), b"ctx");
    assert!(matches!(err, Err(CryptoVaultError::CiphertextTooShort)));
}
