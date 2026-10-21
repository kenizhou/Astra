// SPDX-License-Identifier: GPL-3.0-only
//! `bundle-keygen` and `bundle-plugin`: the publisher's signing key, and the
//! signed `.astra-plugin` tar of a plugin directory. The key file is the raw
//! PKCS#8 the publisher keeps; the bundle lands beside the directory as
//! `<name>.astra-plugin`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ring::signature::KeyPair;

/// `bundle-keygen <key-file>`: mints a publisher key.
pub(crate) fn keygen(key_file: Option<&str>) -> ExitCode {
    let Some(key_file) = key_file else {
        eprintln!("usage: bundle-keygen <key-file>");
        return ExitCode::FAILURE;
    };
    let path = PathBuf::from(key_file);
    if path.exists() {
        eprintln!("bundle-keygen: {key_file} already exists; a publisher key is never overwritten");
        return ExitCode::FAILURE;
    }
    let rng = ring::rand::SystemRandom::new();
    let Ok(pkcs8) = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng) else {
        eprintln!("bundle-keygen: the system CSPRNG refused");
        return ExitCode::FAILURE;
    };
    match std::fs::write(&path, pkcs8.as_ref()) {
        Ok(()) => {
            println!("bundle-keygen: the publisher key is at {}", path.display());
            println!(
                "bundle-keygen: its public half (the manifest's `publisher` field): {}",
                astra_plugin::hex(
                    ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
                        .expect("the key just generated parses")
                        .public_key()
                        .as_ref()
                )
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("bundle-keygen: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `bundle-plugin <dir> <key-file>`: packs and signs a plugin directory.
pub(crate) fn bundle(dir: Option<&str>, key_file: Option<&str>) -> ExitCode {
    let (Some(dir), Some(key_file)) = (dir, key_file) else {
        eprintln!("usage: bundle-plugin <dir> <key-file>");
        return ExitCode::FAILURE;
    };
    let run = || -> Result<PathBuf, String> {
        let key_bytes =
            std::fs::read(key_file).map_err(|e| format!("the key file does not read: {e}"))?;
        let key = ring::signature::Ed25519KeyPair::from_pkcs8(&key_bytes)
            .map_err(|_| "the key file is not an ed25519 PKCS#8".to_string())?;
        let out = Path::new(dir).with_extension("astra-plugin");
        astra_plugin::pack_and_sign(Path::new(dir), &out, &key).map_err(|e| e.to_string())?;
        Ok(out)
    };
    match run() {
        Ok(out) => {
            println!("bundle-plugin: signed bundle at {}", out.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("bundle-plugin: {error}");
            ExitCode::FAILURE
        }
    }
}
