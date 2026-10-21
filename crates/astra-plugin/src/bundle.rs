// SPDX-License-Identifier: GPL-3.0-only
//! The `.astra-plugin` bundle: an uncompressed tar of the plugin's directory
//! plus its publisher identity and signature.
//!
//! The signature is ed25519 over the canonical digest: the sha256 of the
//! sorted `(path, sha256(content))` pairs of every entry except
//! `signature.bin` itself. Verification is tamper-evidence plus identity: the
//! signature must verify against the bundle's `publisher.pub`, and that key
//! must equal the manifest's `publisher` field, so a bundle rewritten in
//! flight, a key that is not the manifest's, or a missing signature in
//! production each answer their own refusal. The fingerprint the consent
//! dialog shows is the sha256 of the publisher key, first eight bytes, hex.
//!
//! This is deliberately not a certificate chain: the trust decision is the
//! human's at install, the same shape as sideloading anywhere.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Component, Path},
};

use ring::{
    digest,
    signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519},
};

use crate::manifest::{ManifestError, PluginManifest};

/// The bundle entries the signature never covers (the signature itself) and
/// always covers (the publisher key, so a swap attack fails the digest).
const SIGNATURE_ENTRY: &str = "signature.bin";
const PUBLISHER_ENTRY: &str = "publisher.pub";

/// Why a bundle was refused. Every variant is a publisher's or a tamperer's
/// condition, not a host condition to retry.
#[derive(Debug)]
pub enum BundleError {
    /// A read or write failed.
    Io(String),
    /// The archive holds no `manifest.toml`.
    MissingManifest,
    /// The manifest inside did not parse.
    BadManifest(ManifestError),
    /// Production installs require a signature; this bundle has none.
    MissingSignature,
    /// The signature does not verify against the bundle's publisher key.
    BadSignature,
    /// The signing key is not the key the manifest names as its publisher.
    PublisherMismatch,
    /// The manifest's entrypoint is not in the bundle.
    MissingEntrypoint(String),
    /// An archive entry's path escapes (a `..`, an absolute or rooted name).
    Traversal(String),
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BundleError::Io(error) => write!(f, "bundle IO failed: {error}"),
            BundleError::MissingManifest => write!(f, "the bundle holds no manifest.toml"),
            BundleError::BadManifest(error) => {
                write!(f, "the bundle's manifest is invalid: {error}")
            }
            BundleError::MissingSignature => write!(f, "the bundle is unsigned"),
            BundleError::BadSignature => write!(f, "the bundle's signature does not verify"),
            BundleError::PublisherMismatch => {
                write!(f, "the signing key is not the manifest's publisher")
            }
            BundleError::MissingEntrypoint(entry) => {
                write!(f, "the bundle does not carry its entrypoint {entry}")
            }
            BundleError::Traversal(name) => write!(f, "the bundle entry {name} escapes its root"),
        }
    }
}

impl Error for BundleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            BundleError::BadManifest(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for BundleError {
    fn from(error: std::io::Error) -> Self {
        BundleError::Io(error.to_string())
    }
}

/// A bundle that verified (or, in dev mode, was accepted unsigned): its parsed
/// manifest and the publisher fingerprint the consent dialog shows.
#[derive(Debug)]
pub struct VerifiedBundle {
    /// The parsed manifest.
    pub manifest: PluginManifest,
    /// The publisher key's fingerprint (empty for an unsigned dev bundle).
    pub publisher_fingerprint: String,
}

/// Lowercase hex, for the manifest's publisher field and the fingerprint.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, b| {
        use fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// The canonical digest: sha256 over the sorted entries, each contributing its
/// path, a NUL, and its content's sha256. `signature.bin` never contributes.
fn canonical_digest(entries: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut context = digest::Context::new(&digest::SHA256);
    for (path, content) in entries {
        if path == SIGNATURE_ENTRY {
            continue;
        }
        context.update(path.as_bytes());
        context.update(&[0]);
        context.update(digest::digest(&digest::SHA256, content).as_ref());
    }
    context.finish().as_ref().to_vec()
}

/// Reads a directory into the entry map: every file, relative path with `/`
/// separators, sorted by the map.
fn read_dir(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>, BundleError> {
    let mut entries = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for child in std::fs::read_dir(&current)? {
            let path = child?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(dir)
                .expect("a child of the walked root")
                .to_string_lossy()
                .replace('\\', "/");
            entries.insert(rel, std::fs::read(&path)?);
        }
    }
    Ok(entries)
}

/// Writes the entry map as an uncompressed tar.
fn write_tar(entries: &BTreeMap<String, Vec<u8>>, out: &Path) -> Result<(), BundleError> {
    let file = std::fs::File::create(out)?;
    let mut builder = tar::Builder::new(file);
    for (path, content) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, &content[..])?;
    }
    builder.finish()?;
    Ok(())
}

/// Packs a directory as an unsigned bundle (dev mode).
///
/// # Errors
///
/// Returns [`BundleError::Io`] when a read or the write fails.
pub fn pack(dir: &Path, out: &Path) -> Result<(), BundleError> {
    write_tar(&read_dir(dir)?, out)
}

/// Packs and signs a directory: the publisher's public key and the signature
/// of the canonical digest join the archive.
///
/// # Errors
///
/// Returns [`BundleError::Io`] when a read or the write fails.
pub fn pack_and_sign(dir: &Path, out: &Path, key: &Ed25519KeyPair) -> Result<(), BundleError> {
    let mut entries = read_dir(dir)?;
    entries.insert(
        PUBLISHER_ENTRY.to_string(),
        key.public_key().as_ref().to_vec(),
    );
    let signature = key.sign(&canonical_digest(&entries)).as_ref().to_vec();
    entries.insert(SIGNATURE_ENTRY.to_string(), signature);
    write_tar(&entries, out)
}

/// Reads an archive into its entry map, refusing any entry whose path escapes.
fn read_tar(bundle: &Path) -> Result<BTreeMap<String, Vec<u8>>, BundleError> {
    let file = std::fs::File::open(bundle)?;
    let mut archive = tar::Archive::new(file);
    let mut entries = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        let safe = Path::new(&path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
        if !safe || path.is_empty() {
            return Err(BundleError::Traversal(path));
        }
        let mut content = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut content)?;
        entries.insert(path, content);
    }
    Ok(entries)
}

/// The manifest's publisher field decoded from hex.
fn publisher_key(manifest: &PluginManifest) -> Result<Vec<u8>, BundleError> {
    let Some(field) = &manifest.publisher else {
        return Err(BundleError::PublisherMismatch);
    };
    let decode = |text: &str| -> Option<Vec<u8>> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
            .collect()
    };
    decode(field)
        .filter(|bytes| bytes.len() == 32)
        .ok_or(BundleError::PublisherMismatch)
}

/// Verifies a bundle without unpacking it. With `require_signature`
/// (production), an unsigned or unverifiable bundle is refused; dev mode
/// (`false`) accepts unsigned bundles. The answer carries the manifest and
/// the publisher fingerprint the consent dialog shows; the entry set stays
/// internal to the unpack that follows.
///
/// # Errors
///
/// Returns the [`BundleError`] naming the refusal: a hostile entry path, a
/// missing or invalid manifest, an unsigned bundle in production, a signature
/// that does not verify, a publisher key that is not the manifest's, or an
/// entrypoint the bundle does not carry.
pub fn verify(bundle: &Path, require_signature: bool) -> Result<VerifiedBundle, BundleError> {
    let entries = read_tar(bundle)?;
    verify_entries(&entries, require_signature)
}

/// Verifies and unpacks a bundle into `target/<manifest id>/`.
///
/// # Errors
///
/// As [`verify`], plus [`BundleError::Io`] when the extraction's writes fail.
pub fn verify_and_unpack(
    bundle: &Path,
    target: &Path,
    require_signature: bool,
) -> Result<VerifiedBundle, BundleError> {
    let entries = read_tar(bundle)?;
    let verified = verify_entries(&entries, require_signature)?;
    unpack_entries(&entries, target, &verified.manifest)?;
    Ok(verified)
}

/// The verification over an entry map: the manifest, the signature and the
/// publisher key when signed, and the entrypoint's presence.
fn verify_entries(
    entries: &BTreeMap<String, Vec<u8>>,
    require_signature: bool,
) -> Result<VerifiedBundle, BundleError> {
    let manifest_bytes = entries
        .get("manifest.toml")
        .ok_or(BundleError::MissingManifest)?;
    let manifest = PluginManifest::parse(&String::from_utf8_lossy(manifest_bytes))
        .map_err(BundleError::BadManifest)?;
    let signed = entries.contains_key(SIGNATURE_ENTRY);
    let mut fingerprint = String::new();
    if signed {
        let public = entries
            .get(PUBLISHER_ENTRY)
            .ok_or(BundleError::PublisherMismatch)?;
        if *public != publisher_key(&manifest)? {
            return Err(BundleError::PublisherMismatch);
        }
        let signature = entries.get(SIGNATURE_ENTRY).expect("checked above");
        let digest = canonical_digest(entries);
        UnparsedPublicKey::new(&ED25519, public.as_slice())
            .verify(&digest, signature)
            .map_err(|_| BundleError::BadSignature)?;
        fingerprint = hex(&digest::digest(&digest::SHA256, public).as_ref()[..8]);
    } else if require_signature {
        return Err(BundleError::MissingSignature);
    }
    // The entrypoint must be in the bundle, whether the manifest names it or
    // the convention does.
    let entrypoint = manifest.entrypoint.clone().unwrap_or_else(|| {
        format!(
            "{}{}",
            manifest
                .id
                .as_str()
                .rsplit('.')
                .next()
                .expect("a parsed id is never empty"),
            std::env::consts::EXE_SUFFIX
        )
    });
    if !entries.contains_key(&entrypoint) {
        return Err(BundleError::MissingEntrypoint(entrypoint));
    }
    Ok(VerifiedBundle {
        manifest,
        publisher_fingerprint: fingerprint,
    })
}

/// The extraction: the verified entries land under `target/<manifest id>/`,
/// the signing pair excluded (it is metadata, not the plugin's files).
fn unpack_entries(
    entries: &BTreeMap<String, Vec<u8>>,
    target: &Path,
    manifest: &PluginManifest,
) -> Result<(), BundleError> {
    let root = target.join(manifest.id.as_str());
    for (path, content) in entries {
        if path == SIGNATURE_ENTRY || path == PUBLISHER_ENTRY {
            continue;
        }
        let destination = root.join(path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(destination, content)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "bundle_tests.rs"]
mod tests;
