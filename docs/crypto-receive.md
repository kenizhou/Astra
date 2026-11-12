# Crypto receive: the contract

**Scope.** Decrypting incoming encrypted mail through the reading pipeline
and surfacing the security state. The receive half of the crypto feature
(SP4c); the send half (SP4b+ECS) ships separately. This first sub-slice
(SP4c-1) covers OpenPGP PGP/MIME encrypted mail. Binding on the reading
pipeline and the crypto store. The engine is crypto-ignorant by design: it
transports bytes, Astra interprets the crypto framing (the send precedent's
mirror).

## The path

1. The reading pipeline fetches the raw RFC5322 source through the existing
   `engine.message_source` read (cache-first, offline-capable). No new
   engine seam: receive rides the byte hand-off the decode seam already
   consumes.
2. A built-in `MessageDecoder` (the crypto decoder) calls the receive
   orchestrator (`astra-crypto-store::receive_openpgp`), which detects and
   unwraps the PGP/MIME `multipart/encrypted` framing (protocol
   `application/pgp-encrypted`), resolves the account's private OpenPGP key,
   and decrypts through `OpenpgpBackend::decrypt_with_signers` directly
   (offering the keystore signers and the collected certs, so an embedded
   signature verifies against known peers).
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

## Known gaps, recorded rather than left silent

- This sub-slice is OpenPGP PGP/MIME encrypted only. Inline PGP armor and
  multipart/signed (detached signatures) are follow-ups; S/MIME receive is
  SP4c-2; Autocrypt consume, CRL fetching, and the W9 password flow are
  SP4c-3; encrypted drafts at rest are SP4c-4.
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
| The result rows (the first writer of message_crypto_results) | ✅ | `receive.rs`, `results.rs` |
| The reading integration: legs, banner, restored subject, hidden chip | ✅ | `astra-app/src/crypto_decode.rs`, `reading.rs` |
| The composite decoder (crypto verdict precedence, plugin fall-through) | ✅ | `client/desktop/src/decode.rs` |
| Fail closed: no key / failure / tamper never renders ciphertext | ✅ | the closed arms + the structural seam clear |
| The live proof (send encrypted, open, decrypt, the no-key arm) | ✅ | `tests_live_harness/crypto_receive.rs` |
| S/MIME receive, inline armor, detached signatures, drafts at rest | N/A | SP4c-2/3/4, the gaps above |
| Key management, the token registry, the native security UI | N/A | SP4d |