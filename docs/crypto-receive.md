# Crypto receive: the contract

**Scope.** Decrypting incoming encrypted mail through the reading pipeline
and surfacing the security state. The receive half of the crypto feature
(SP4c); the send half (SP4b+ECS) ships separately. SP4c-1 covers OpenPGP
PGP/MIME encrypted mail; SP4c-2 covers S/MIME enveloped-data (including the
nested sign-then-encrypt shape). Binding on the reading pipeline and the
crypto store. The engine is crypto-ignorant by design: it transports bytes,
Astra interprets the crypto framing (the send precedent's mirror).

## The path

1. The reading pipeline fetches the raw RFC5322 source through the existing
   `engine.message_source` read (cache-first, offline-capable). No new
   engine seam: receive rides the byte hand-off the decode seam already
   consumes.
2. A built-in `MessageDecoder` (the crypto decoder) calls the unified
   receive dispatcher (`astra-crypto-store::receive`), which sniffs the
   framing once and routes: PGP/MIME `multipart/encrypted` (protocol
   `application/pgp-encrypted`) to `receive_openpgp`, S/MIME
   `application/pkcs7-mime; smime-type=enveloped-data` to `receive_smime`.
   The OpenPGP path resolves the account's private key and decrypts through
   `OpenpgpBackend::decrypt_with_signers` directly (offering the keystore
   signers and the collected certs, so an embedded signature verifies
   against known peers).
3. The orchestrator writes `message_crypto_results` (its first writer): the
   crypto kind, the decrypt state (ok / no-key / failed), the signature
   state, the signer fingerprint and email, keyed by (account, message) with
   the linkage enforced in Rust.
4. The decrypted legs re-run the identical sanitise pipeline; the restored
   protected subject replaces the outer placeholder; the security banner
   states the verdict; the ciphertext pseudo-attachment chip is hidden.

## The fail-closed states

The crypto verdict is fail CLOSED (the plugin decoder's fail-open does not
apply to it). A recognised message that cannot be decrypted never renders
ciphertext or the outer framing:

- **Not encrypted** (not the PGP/MIME framing): the rendering is left
  untouched and the plugin decoder may still serve it; no result row.
- **No key** (the account holds no private key that decrypts it): a
  "cannot decrypt" banner, no plaintext, a `no-key` result row.
- **Failed** (a tampered ciphertext, a bad signature, a wrong key, or
  recognised framing whose ciphertext will not extract): a "cannot decrypt"
  banner, no plaintext, a `failed` result row.
- **Decrypted**: the legs and the signature verdict (verified, or
  unverified for an unknown signer), the restored subject, an `ok` row.

The composite decoder consults the crypto decoder first; any verdict it
recognises takes precedence and the plugin never sees that message. Only
the not-encrypted answer falls through to the plugin. The reading seam
clears the store's own legs on every crypto verdict, structurally, so the
outer framing never renders beside it.

## The detection boundary

Protocol parsing (IMAP fetch, blob caching, MIME structure for display) is
the engine's. Crypto-envelope detection and interpretation are Astra's
crypto layer, exactly as the send gate's MIME framing lives in Astra. The
engine tree stays untouched.

## The S/MIME path (SP4c-2)

`receive_smime` unwraps the enveloped-data, resolves the account's S/MIME
private leaf (the p12-imported cert; a token-held key routes to the W6 token
path), and decrypts through `SmimeBackend::decrypt`. A sign-then-encrypt
message's plaintext is itself an opaque CMS SignedData; the eContent is
extracted and verified through `verify_with_context` with the inputs
resolved from the store: the From address (the From↔SAN binding), the trust
anchors, the intermediates (stored merged with the embedded cert set,
deduped by fingerprint), the CRL cache, and the signer's latest trust
decision. The discovered signer leaf is harvested (never auto-trusted).

The signature state ladder is precise:

- Chain invalid (a missing or untrusted anchor, an expired cert, a revoked
  cert) → **Invalid**.
- Chain valid but the From address does not match the signer certificate's
  SAN → **Mismatch**.
- Chain valid, identity matches, and the signer trust displays as verified
  (Verified or Personal) → **ValidVerified**.
- Otherwise (chains to a trusted anchor, signer not explicitly accepted) →
  **ValidUnverified**.

The result row carries the `standard` discriminator and the chain fields
(`chain_valid`, `revocation_state`, the granular `failure_reason` and
`revocation_reason`), added by a backward-compatible table-rebuild
migration. The banner states the verdict (the revoked case is
distinguished); the no-key / failed arms fail closed exactly as OpenPGP.
The p12 intermediates are stored `key_type='intermediate'`, never through
the keystore's cert path (which would leak them into the anchor set).

## Known gaps, recorded rather than left silent

- Encrypted receive only, per standard: OpenPGP PGP/MIME (SP4c-1) and S/MIME
  enveloped-data (SP4c-2). Inline PGP armor and the detached-signature shapes
  (PGP multipart/signed, S/MIME multipart/signed and opaque signed-data) are
  follow-ups; Autocrypt consume, CRL fetching, and the W9 password flow are
  SP4c-3; encrypted drafts at rest are SP4c-4.
- S/MIME revocation is cache-only: the CRL cache is empty until the SP4c-3
  fetcher lands, so revocation reads unchecked in practice (a revoked cert
  still hard-fails the chain when a CRL is present). The chain's Stale
  revocation state persists as unchecked until the fetcher lands.
- No key-management surface yet (SP4d): the live proof seeds the recipient
  key programmatically. A minimal import path is the load-bearing follow-up
  that makes receive usable beyond tests.
- A legacy signature carrying only an Issuer KeyID (no IssuerFingerprint
  subpacket) can verify yet report no signer fingerprint, so it labels
  NotSigned rather than ValidVerified. A degradation, never a false
  "verified"; modern clients emit IssuerFingerprint.
- The CLI `messages read` still reads the engine body directly (encrypted
  mail answers empty there); the receive path is the desktop reading
  pipeline. A CLI security field is a named follow-up.
- The token registry (cryptoki, openpgp-card) and the native security UI are
  SP4d.

## Matrix

| Rule | Status | Where |
|---|---|---|
| OpenPGP PGP/MIME receive: detect, unwrap, decrypt, verify-state | ✅ | `astra-crypto-store/src/receive.rs` |
| S/MIME enveloped receive: unwrap, decrypt, chain verify, the state ladder | ✅ | `astra-crypto-store/src/receive_smime.rs` |
| The unified receive dispatcher (PGP/MIME vs S/MIME routing) | ✅ | `receive.rs` |
| The result rows (the first writer of message_crypto_results) | ✅ | `receive.rs`, `receive_smime.rs`, `results.rs` |
| The chain-field schema (standard, revocation 'stale', table rebuild) | ✅ | `migrations/0004_smime_receive.sql` |
| The reading integration: legs, banner, restored subject, hidden chip | ✅ | `astra-app/src/crypto_decode.rs`, `reading.rs` |
| The composite decoder (crypto verdict precedence, plugin fall-through) | ✅ | `client/desktop/src/decode.rs` |
| Fail closed: no key / failure / tamper never renders ciphertext | ✅ | the closed arms + the structural seam clear |
| The live proofs (send encrypted, open, decrypt, the negative arms) | ✅ | `tests_live_harness/crypto_receive.rs`, `crypto_receive_smime.rs` |
| Inline armor, detached signatures, CRL fetch, drafts at rest | N/A | SP4c-3/4, the gaps above |
| Key management, the token registry, the native security UI | N/A | SP4d |