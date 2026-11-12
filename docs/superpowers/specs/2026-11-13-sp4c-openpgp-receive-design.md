# SP4c-1 design: OpenPGP receive through the reading pipeline

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** The first sub-slice of the crypto receive half (SP4c): decrypt
incoming OpenPGP PGP/MIME mail through the reading pipeline and surface the
security state. The send half (SP4b+ECS) is shipped and live-proven; its
live proof hand-rolls the extraction-and-decrypt tail that this slice turns
into the product path. The decrypt/verify primitives already exist
(`OpenpgpBackend::decrypt_with_signers`, "the receive path's entry point");
what is missing is the orchestrator and the reading integration. No engine
change: receive rides the existing `engine.message_source` raw-byte read
and the `MessageDecoder` seam; detection lives in Astra's crypto layer, the
send precedent's mirror (the engine transports bytes, Astra interprets the
crypto framing).

## D1: the receive orchestrator (astra-crypto-store)

A new receive module beside `wrap.rs` (the send model), cap-exempt like the
rest of the crate. `receive_openpgp(account, message_id, source)`:

1. **Detect + unwrap** the PGP/MIME framing from the raw RFC5322 source:
   `multipart/encrypted` with protocol `application/pgp-encrypted` (the
   RFC 3156 shape the send half wraps). Extract the ciphertext part. A new
   unwrap helper (the inverse of `pgp_mime::wrap_encrypted`); the existing
   `mime::split_message` is reusable. Not the crypto framing → answer
   "not encrypted" (the decoder leaves the store rendering untouched).
2. **Resolve the key**: the account's private OpenPGP key from the crypto
   store (`SqliteKeyStore`, the private rows). No key → the `no-key` state
   (fail closed, never render ciphertext).
3. **Decrypt + verify**: call `decrypt_with_signers` directly (NOT the
   trait `decrypt`, which drops the weak/unverifiable channels), offering
   the keystore signers + the collected certs. Map the `DecryptOutcome`:
   the payload, `weak_warning`, `unverifiable_signature` → the signature
   state (`UnknownKey` for an unknown signer, the six-state vocabulary).
4. **Persist the result**: write `message_crypto_results` (the schema is
   ready, zero writers today; this is its first writer): crypto_kind,
   decrypt_state (ok/no-key/failed), signature_state, signer fp/email, the
   PK (account_id, message_id) linkage enforced in Rust.
5. **Restore the protected subject**: the real Subject rides inside the
   protected headers (the send half's `wrap_protected_headers`); the
   orchestrator returns it so the snapshot shows the real subject, not the
   outer `...`.

The result is a neutral type (the decrypted legs + the security state + the
restored subject), engine-free, mirroring `wrap.rs`'s dependency direction
(the store crate does not depend on astra-app).

## D2: the reading integration (astra-app + desktop)

A built-in `MessageDecoder` (a new astra-app module; `reading.rs` is near
the cap, so a sibling, not growth) that calls the orchestrator and maps the
result to `DecodeOutcome { html, plain, banner }`:

- The decrypted plain/html legs re-run the IDENTICAL `render_html_leg`
  sanitise pipeline (the decode seam already does this).
- The banner states the security state: decrypted+verified, decrypted+
  unverified (unknown signer), cannot-decrypt (no key), failed. Fail
  CLOSED: no key or a failure renders the banner, never the ciphertext;
  the plugin decoder's fail-open does NOT apply to built-in crypto verdicts.
- Hide the ciphertext pseudo-attachment chip (the `smime.p7m` /
  `encrypted.asc` attachment), the `hide_invitation_parts` precedent.

The desktop boot installs a COMPOSITE decoder (the built-in crypto
consulted alongside the existing `PluginDecoder`; today only one decoder is
installed). The crypto verdict takes precedence for a message it
recognises; the plugin decoder still serves its own messages.

## D3: the contract

A new `docs/crypto-receive.md` (+ zh): the receive path, the fail-closed
states, the detection boundary (Astra interprets the framing, the engine
transports bytes), the result-row linkage, the protected-subject restore.
The capabilities S/MIME/OpenPGP rows move: the OpenPGP receive half lands
(the row's note names what remains: S/MIME receive, the security UI, key
management). A changelog fragment lands. The porting-backlog is unaffected
(this is a C-cluster partial row, not a kylins addition).

## Known gaps recorded, not hidden

- This sub-slice is OpenPGP PGP/MIME encrypted only. Deferred within SP4c:
  inline PGP armor, multipart/signed (detached signatures), S/MIME receive
  (SP4c-2), Autocrypt consume + CRL fetching + the W9 password flow
  (SP4c-3), encrypted drafts at rest (SP4c-4).
- No key-management surface yet (SP4d): the live proof seeds the recipient
  key programmatically, the send proof's own discipline. A minimal import
  path is the load-bearing follow-up that makes receive usable beyond tests.
- The CLI `messages read` still reads the engine body directly (encrypted
  mail answers empty there); the receive path is the desktop reading
  pipeline. A CLI security field is a named follow-up.
- The token registry (cryptoki, openpgp-card) is SP4d; a token key fails
  TokenNotPresent today.

## Test-first

Every behaviour lands with its red. Offline (the `reading_decode_tests.rs`
fake pattern, `FakeProvider::with_sources`): a PGP/MIME encrypted source
decrypts to the words with a verified banner; no key → the no-key banner
and no plaintext; an unknown signer → unverified; a tampered ciphertext →
failed (fail closed); the protected subject is restored; the ciphertext
chip is hidden; a non-encrypted source is untouched; the result row lands
with the right states. The orchestrator unit tests pin the unwrap, the key
resolution, the state mapping, and the result-row linkage. ONE gated live
harness proof (the send proof's tail automated): seed astra2's key, send
encrypted astra1→astra2 through the engine seam, open the delivered row,
assert the snapshot carries the words + the verified banner + the restored
subject + a result row, and the ciphertext never renders; the no-key
negative arm; withdraw all copies. No eval/pipeline bump (the model-facing
surface is unchanged; the AI incoming pass sees no text for encrypted mail,
fail-soft, recorded).