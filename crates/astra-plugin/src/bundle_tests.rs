// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the plugin bundle: the tar round trip, the canonical digest, and
//! the ed25519 signature. Tamper-evidence and identity are the contract: a
//! rewritten byte, a wrong key, or a missing signature in production each
//! answer their own refusal.

use std::path::PathBuf;

use ring::signature::{Ed25519KeyPair, KeyPair};

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "astra-plugin-bundle-{}-{}",
        name,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// A key pair for one test.
fn mint_key() -> Ed25519KeyPair {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("generated");
    Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("parsed")
}

/// A plugin directory: a manifest naming the key's public half, one binary,
/// one page.
fn plugin_dir(name: &str, key: &Ed25519KeyPair) -> PathBuf {
    let dir = scratch(name).join("dev.astra.keys");
    std::fs::create_dir_all(dir.join("assets")).expect("dirs");
    std::fs::write(
        dir.join("manifest.toml"),
        format!(
            r#"
id = "dev.astra.keys"
name = "Keys"
version = "0.1.0"
api_level = 1
aom_version = 1
methods = ["plugin.dev.astra.keys.ping"]
permissions = []
publisher = "{}"
privacy_policy_url = "https://example.test/privacy"
"#,
            hex_of(key)
        ),
    )
    .expect("manifest");
    std::fs::write(dir.join("keys.exe"), b"\x7fbin").expect("binary");
    std::fs::write(dir.join("assets/keys.html"), "<p>keys</p>").expect("page");
    dir
}

/// The publisher's public key, lowercase hex.
fn hex_of(key: &Ed25519KeyPair) -> String {
    hex(key.public_key().as_ref())
}

#[test]
fn a_signed_bundle_round_trips() {
    let key = mint_key();
    let dir = plugin_dir("round-trip", &key);
    let bundle = scratch("round-trip").join("keys.astra-plugin");
    pack_and_sign(&dir, &bundle, &key).expect("packed and signed");

    let target = scratch("round-trip-out");
    let verified = verify_and_unpack(&bundle, &target, true).expect("verified");
    assert_eq!(verified.manifest.id.as_str(), "dev.astra.keys");
    assert_eq!(
        verified.publisher_fingerprint.len(),
        16,
        "a short fingerprint"
    );
    assert_eq!(
        verified.manifest.privacy_policy_url.as_deref(),
        Some("https://example.test/privacy")
    );
    assert_eq!(
        std::fs::read(target.join("dev.astra.keys").join("assets/keys.html")).expect("unpacked"),
        b"<p>keys</p>"
    );
}

#[test]
fn a_flipped_byte_fails_verification() {
    let key = mint_key();
    let dir = plugin_dir("tampered", &key);
    let bundle = scratch("tampered").join("keys.astra-plugin");
    pack_and_sign(&dir, &bundle, &key).expect("packed");
    // Rewrite one payload byte inside the archive without re-signing.
    let mut bytes = std::fs::read(&bundle).expect("read");
    let anchor = bytes
        .windows(7)
        .position(|w| w == b"<p>keys")
        .expect("the page's bytes are in the archive");
    bytes[anchor] = b'<';
    bytes[anchor + 1] = b'p';
    bytes[anchor + 2] = b'B'; // "<p>B" for "<p>k"
    std::fs::write(&bundle, bytes).expect("written");
    let error = verify_and_unpack(&bundle, &scratch("tampered-out"), true)
        .expect_err("the tamper must fail");
    assert!(matches!(error, BundleError::BadSignature));
}

#[test]
fn a_mismatched_publisher_key_fails() {
    let key = mint_key();
    let other = mint_key();
    let dir = plugin_dir("mismatch", &other); // the manifest names the other key
    let bundle = scratch("mismatch").join("keys.astra-plugin");
    pack_and_sign(&dir, &bundle, &key).expect("packed"); // but this key signed
    let error = verify_and_unpack(&bundle, &scratch("mismatch-out"), true)
        .expect_err("the publisher mismatch must fail");
    assert!(matches!(error, BundleError::PublisherMismatch));
}

#[test]
fn production_requires_a_signature_and_dev_mode_waves_it() {
    let key = mint_key();
    let dir = plugin_dir("unsigned", &key);
    let bundle = scratch("unsigned").join("keys.astra-plugin");
    pack(&dir, &bundle).expect("packed unsigned");
    assert!(matches!(
        verify_and_unpack(&bundle, &scratch("unsigned-prod"), true),
        Err(BundleError::MissingSignature)
    ));
    assert!(
        verify_and_unpack(&bundle, &scratch("unsigned-dev"), false).is_ok(),
        "dev mode loads unsigned"
    );
}

#[test]
fn a_bundle_missing_its_entrypoint_is_refused() {
    let key = mint_key();
    let dir = plugin_dir("entrypoint", &key);
    // The manifest names a binary the bundle does not carry.
    let manifest = std::fs::read_to_string(dir.join("manifest.toml")).expect("manifest");
    std::fs::write(
        dir.join("manifest.toml"),
        format!("{manifest}entrypoint = \"ghost.exe\"\n"),
    )
    .expect("rewritten");
    std::fs::remove_file(dir.join("keys.exe")).expect("removed");
    let bundle = scratch("entrypoint").join("keys.astra-plugin");
    pack_and_sign(&dir, &bundle, &key).expect("packed");
    let error = verify_and_unpack(&bundle, &scratch("entrypoint-out"), true)
        .expect_err("the missing entrypoint must fail");
    assert!(matches!(error, BundleError::MissingEntrypoint(_)));
}

#[test]
fn a_traversal_entry_is_refused() {
    let key = mint_key();
    let dir = plugin_dir("traversal", &key);
    let bundle = scratch("traversal").join("keys.astra-plugin");
    pack_and_sign(&dir, &bundle, &key).expect("packed");
    // Graft a hostile entry onto the archive after signing, built by hand
    // (the tar crate itself refuses `..` paths at write time, which is its
    // own good defense): it alters the digest, so the answer must be a
    // refusal either way.
    let mut bytes = std::fs::read(&bundle).expect("read");
    let extra = {
        let mut header = [0u8; 512];
        header[..13].copy_from_slice(b"../escape.txt");
        header[100..108].copy_from_slice(b"0000644\0");
        header[124..136].copy_from_slice(b"00000000005\0");
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].copy_from_slice(b"        ");
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
        let mut entry = header.to_vec();
        entry.extend_from_slice(b"owned");
        entry.resize(512 + 512, 0);
        entry
    };
    // Insert before the trailing zero blocks (two 512-byte blocks end a tar).
    let tail = bytes.split_off(bytes.len() - 1024);
    bytes.extend_from_slice(&extra);
    bytes.extend_from_slice(&tail);
    std::fs::write(&bundle, bytes).expect("written");
    assert!(verify_and_unpack(&bundle, &scratch("traversal-out"), true).is_err());
    assert!(
        !scratch("traversal-out").join("escape.txt").exists(),
        "nothing escaped the target"
    );
}
