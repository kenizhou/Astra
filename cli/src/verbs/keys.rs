// SPDX-License-Identifier: GPL-3.0-only
//! The key verbs over `crypto.sqlite`, the store the desktop's key import
//! and collected-key review fill: the public reads, the public-only
//! import, and the promote/reject review actions. PUBLIC material only:
//! private-key import stays desktop, because the vault master key lives
//! in the OS keyring and the CLI contract forbids opening it; a
//! secret-bearing input is refused with a pointer to the desktop path.
//!
//! The store opens lazily, the engine-store pattern: no key verb touches
//! the engine, and a cold data dir answers the reads empty. The promote
//! and reject orchestration replicates the app's (astra-app's collected
//! review): the two crates do not depend on each other, so the pairing is
//! spelled twice and pinned by both test suites, the closed action set's
//! own precedent. The trust write is load-bearing: a promote without its
//! decision leaves the send resolver Undecided.

use std::io::Read;
use std::path::Path;

use astra_crypto_store::{CollectedKeyRow, CryptoKeyRow, CryptoStore};
use engine_api::AccountId;
use serde_json::{json, Value};

use super::parse::{data_value, flag_present, non_empty, one_value, owned_value, required, usage};
use super::verb::{KeyImportArgs, KeyPromoteArgs, KeyRejectArgs};
use super::{Answer, ParseFailure};
use crate::envelope::Code;

/// The crypto store's file name under the data dir, the same file the
/// desktop host opens (`boot.rs`).
const CRYPTO_STORE_FILE: &str = "crypto.sqlite";

/// The standards the key verbs answer, the two the app's key surfaces
/// merge. The store's third word (`sm`) has no CLI surface.
const STANDARDS: &[&str] = &["openpgp", "smime"];

/// The secret-material refusal's pointer: the private path needs the
/// vault master key, which lives in the OS keyring the CLI never opens.
const SECRET_POINTER: &str = "the input carries secret key material; private \
     key import stays desktop: open the account in Astra Mail and use the \
     Settings Keys import, where the vault master key lives";

/// The unregistered-account error: `crypto_keys` references the store's
/// account registry, which only the app fills (it knows the account's own
/// email; the CLI's index holds ids alone).
const UNREGISTERED: &str = "the app has not registered this account's key \
     store; open it in Astra Mail first";

/// Opens the crypto store beside the engine store, creating and migrating
/// it when absent (the engine open's own benign seam).
fn open(data_dir: &Path) -> Result<CryptoStore, (Code, String)> {
    CryptoStore::open(&data_dir.join(CRYPTO_STORE_FILE)).map_err(|err| {
        (
            Code::Runtime,
            format!("the key store could not be opened: {err}"),
        )
    })
}

/// Maps a store write failure: the foreign-key refusal names the missing
/// account registration, every other failure rides verbatim.
fn write_err(account: &AccountId, err: &str) -> (Code, String) {
    if err.contains("FOREIGN KEY") {
        (
            Code::Runtime,
            format!("{UNREGISTERED} (account {})", account.as_str()),
        )
    } else {
        (
            Code::Runtime,
            format!("the key store refused the write: {err}"),
        )
    }
}

/// The shaped public view of one keystore row: the fields an agent acts
/// on, never the blob and never a secret.
fn shaped_key(row: &CryptoKeyRow) -> Value {
    json!({
        "fingerprint": row.fingerprint,
        "email": row.email,
        "standard": row.standard,
        "key_type": row.key_type,
        "origin": row.origin,
        "has_private": row.has_private,
        "created_at": row.created_at,
        "expires_at": row.expires_at,
    })
}

/// The shaped view of one staging row: the review fields, never the blob.
fn shaped_collected(row: &CollectedKeyRow) -> Value {
    json!({
        "id": row.id,
        "peer_email": row.peer_email,
        "standard": row.standard,
        "fingerprint": row.fingerprint,
        "source": row.source,
        "seen_at": row.seen_at,
    })
}

/// Lists the account's keys across both standards, public view only:
/// OpenPGP rows first, then S/MIME, each ordered by fingerprint (the
/// app's list merge).
pub(crate) fn list(data_dir: &Path, account: &AccountId) -> Answer {
    let store = open(data_dir)?;
    let mut rows =
        astra_crypto_store::list_crypto_keys_for_account(&store, account.as_str(), "openpgp")
            .map_err(|err| read_err(&err))?;
    rows.extend(
        astra_crypto_store::list_crypto_keys_for_account(&store, account.as_str(), "smime")
            .map_err(|err| read_err(&err))?,
    );
    let keys: Vec<Value> = rows.iter().map(shaped_key).collect();
    Ok(json!({ "keys": keys }))
}

/// Lists the account's staged collected keys across both standards,
/// newest first (the store's `seen_at DESC, id DESC` ordering merged
/// across the standards, the app's review list).
pub(crate) fn collected(data_dir: &Path, account: &AccountId) -> Answer {
    let store = open(data_dir)?;
    let mut rows =
        astra_crypto_store::list_collected_keys_for_account(&store, account.as_str(), "openpgp")
            .map_err(|err| read_err(&err))?;
    rows.extend(
        astra_crypto_store::list_collected_keys_for_account(&store, account.as_str(), "smime")
            .map_err(|err| read_err(&err))?,
    );
    rows.sort_by(|a, b| b.seen_at.cmp(&a.seen_at).then(b.id.cmp(&a.id)));
    let keys: Vec<Value> = rows.iter().map(shaped_collected).collect();
    Ok(json!({ "keys": keys }))
}

/// The read failure's mapping.
fn read_err(err: &str) -> (Code, String) {
    (
        Code::Runtime,
        format!("the key store could not be read: {err}"),
    )
}

/// Imports a peer's public cert, sniffing the standard or pinning it when
/// the caller named one. The default unverified trust rides with the
/// import (the store records it). `--preview` names the action without
/// reading the store; a secret-bearing input is refused pointing at the
/// desktop private path.
pub(crate) fn import(
    data_dir: &Path,
    account: &AccountId,
    args: &KeyImportArgs,
    preview: bool,
) -> Answer {
    if preview {
        return Ok(json!({
            "action": "import_key",
            "account": args.account,
            "bytes": args.bytes.len(),
            "source": args.source,
            "standard": args.standard,
        }));
    }
    let store = open(data_dir)?;
    let landed = match &args.standard {
        Some(standard) => astra_crypto_store::import_public_cert_as(
            &store,
            account.as_str(),
            standard,
            &args.bytes,
        ),
        None => astra_crypto_store::import_public_cert(&store, account.as_str(), &args.bytes),
    };
    let row = landed.map_err(|err| {
        if err.contains("secret key material") {
            (Code::Runtime, SECRET_POINTER.to_owned())
        } else {
            write_err(account, &err)
        }
    })?;
    Ok(json!({ "key": shaped_key(&row), "state": "imported" }))
}

/// Promotes a staged key into the keystore AND records the acceptance,
/// the app orchestration's CLI copy: the effective email is read before
/// the promote drains staging, the promote lands the key (never
/// downgrading a standing row), and the trust write makes it send-usable.
/// A verified choice always upgrades; an unverified one records only when
/// no decision stands.
pub(crate) fn promote(
    data_dir: &Path,
    account: &AccountId,
    args: &KeyPromoteArgs,
    preview: bool,
) -> Answer {
    if preview {
        return Ok(json!({
            "action": "promote_key",
            "account": args.account,
            "standard": args.standard,
            "fingerprint": args.fingerprint,
            "email": args.email,
            "verified": args.verified,
        }));
    }
    let store = open(data_dir)?;
    // The address the acceptance binds: the caller's email, or the staged
    // row's peer email when none is named. Read before the promote, which
    // drains the staging rows.
    let effective_email = match &args.email {
        Some(email) => Some(email.clone()),
        None => astra_crypto_store::list_collected_keys_for_fingerprint(
            &store,
            account.as_str(),
            &args.standard,
            &args.fingerprint,
        )
        .map_err(|err| read_err(&err))?
        .into_iter()
        .next()
        .and_then(|row| row.peer_email),
    };
    let row = astra_crypto_store::promote_collected_key(
        &store,
        account.as_str(),
        &args.standard,
        &args.fingerprint,
        args.email.as_deref(),
    )
    .map_err(|err| write_err(account, &err))?;
    astra_crypto_store::record_promote_trust(
        &store,
        account.as_str(),
        effective_email.as_deref(),
        &args.standard,
        &args.fingerprint,
        args.verified,
    )
    .map_err(|err| write_err(account, &err))?;
    Ok(json!({ "key": shaped_key(&row), "state": "promoted" }))
}

/// Rejects a staged key, the app orchestration's CLI copy: the staging
/// row's own peer email stands in when the caller names none, the row is
/// removed, and a rejected decision is recorded (an empty address when
/// neither stands, the unknown-key audit precedent).
pub(crate) fn reject(
    data_dir: &Path,
    account: &AccountId,
    args: &KeyRejectArgs,
    preview: bool,
) -> Answer {
    if preview {
        return Ok(json!({
            "action": "reject_key",
            "account": args.account,
            "id": args.id,
            "standard": args.standard,
            "fingerprint": args.fingerprint,
            "email": args.email,
        }));
    }
    let store = open(data_dir)?;
    let fallback = if args.email.is_none() {
        astra_crypto_store::list_collected_keys_for_fingerprint(
            &store,
            account.as_str(),
            &args.standard,
            &args.fingerprint,
        )
        .map_err(|err| read_err(&err))?
        .into_iter()
        .find(|row| row.id == args.id)
        .and_then(|row| row.peer_email)
    } else {
        None
    };
    astra_crypto_store::remove_collected_key(&store, args.id)
        .map_err(|err| write_err(account, &err))?;
    let peer = args.email.clone().or(fallback).unwrap_or_default();
    astra_crypto_store::put_trust_decision(
        &store,
        account.as_str(),
        &peer,
        &args.standard,
        &args.fingerprint,
        "rejected",
        None,
    )
    .map_err(|err| write_err(account, &err))?;
    Ok(json!({ "id": args.id, "state": "rejected" }))
}

/// Reads `keys import`'s arguments, resolving the material from exactly
/// one of `--plain`, `--file`, or stdin, the drafts one-source discipline
/// over raw bytes (a binary DER cert rides `--file` or stdin).
pub(super) fn import_args(
    args: &[String],
    stdin: &mut dyn Read,
) -> Result<KeyImportArgs, ParseFailure> {
    let account = required(args, "--account")?;
    let standard = owned_value(args, "--standard")?
        .map(validate_standard)
        .transpose()?;
    let (bytes, source) = match (data_value(args, "--plain")?, one_value(args, "--file")?) {
        (Some(_), Some(_)) => {
            return Err(usage(
                "the material comes from one source: --plain, --file, or stdin",
            ))
        }
        (Some(text), None) => (text.as_bytes().to_vec(), "arg"),
        (None, Some(path)) => (
            std::fs::read(Path::new(path))
                .map_err(|err| usage(&format!("the --file material could not be read: {err}")))?,
            "file",
        ),
        (None, None) => {
            let mut bytes = Vec::new();
            stdin
                .read_to_end(&mut bytes)
                .map_err(|err| usage(&format!("the stdin material could not be read: {err}")))?;
            (bytes, "stdin")
        }
    };
    Ok(KeyImportArgs {
        account,
        bytes,
        source,
        standard,
    })
}

/// Reads `keys promote`'s arguments.
pub(super) fn promote_args(args: &[String]) -> Result<KeyPromoteArgs, ParseFailure> {
    Ok(KeyPromoteArgs {
        account: required(args, "--account")?,
        standard: validate_standard(required(args, "--standard")?)?,
        fingerprint: non_empty(required(args, "--fingerprint")?, "--fingerprint")?,
        email: owned_value(args, "--email")?,
        verified: flag_present(args, "--verified"),
    })
}

/// Reads `keys reject`'s arguments.
pub(super) fn reject_args(args: &[String]) -> Result<KeyRejectArgs, ParseFailure> {
    let id = required(args, "--id")?
        .parse::<i64>()
        .map_err(|_| usage("the --id flag wants the staging row's number"))?;
    Ok(KeyRejectArgs {
        account: required(args, "--account")?,
        id,
        standard: validate_standard(required(args, "--standard")?)?,
        fingerprint: non_empty(required(args, "--fingerprint")?, "--fingerprint")?,
        email: owned_value(args, "--email")?,
    })
}

/// A `--standard` word from the closed set: the two standards the key
/// surfaces answer, no other word reaches the store.
fn validate_standard(word: String) -> Result<String, ParseFailure> {
    if STANDARDS.contains(&word.as_str()) {
        Ok(word)
    } else {
        Err(usage(&format!(
            "unknown --standard {word}; the standards are {}",
            STANDARDS.join(" and ")
        )))
    }
}
