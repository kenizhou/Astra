# SP4c-2 design: S/MIME receive through the reading pipeline

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** The second sub-slice of the crypto receive half (SP4c): decrypt
and verify incoming S/MIME encrypted mail through the reading pipeline,
mirroring SP4c-1 (OpenPGP receive, shipped). Scoped to ENCRYPTED S/MIME
(`application/pkcs7-mime; smime-type=enveloped-data`, including the nested
sign-then-encrypt shape the send gate emits); the sign-only shapes
(multipart/signed detached, opaque signed-data) are deferred with the other
detached-signature work, exactly as SP4c-1 deferred PGP multipart/signed.
The S/MIME verify foundation (`verify_with_context`, the chain validation,
the cert/trust/CRL store queries) already exists and is tested; this slice
is the framing unwrap, the verify-input resolution (the "G5 orchestrator"
the backend doc assigns), and the reading integration. No engine change:
receive rides `engine.message_source`; the crypto framing stays in Astra's
layer.

## D1: the framing unwrap and the body extraction

A new `smime_mime` module in astra-crypto-core (mirroring `pgp_mime`, pub so
the store orchestrator calls it): detect and unwrap
`application/pkcs7-mime` (and `application/x-pkcs7-mime`) with
`smime-type=enveloped-data`, base64-decoding the body to the EnvelopedData
DER. A recognised-but-unextractable framing fails closed (the SP4c-1
`is_pgp_encrypted` precedent). A `From`-extraction helper in core (the
store cannot read `header_value` today, it is pub(crate)).

After `SmimeBackend::decrypt`, a sign-then-encrypt message's plaintext is
itself an opaque CMS SignedData; a new pub `extract_signed_content` helper
in astra-crypto-smime recovers the inner body bytes (the eContent) and a
SignedData sniff, so the orchestrator restores the subject and legs. CMS
DER parsing stays in the smime crate, never the store.

## D2: the orchestrator (astra-crypto-store)

A sibling `receive_smime.rs` mirroring `receive.rs`, and a thin unified
`receive(store, account, message, source)` entry that sniffs the framing
once (PGP/MIME vs S/MIME enveloped) and routes; `CryptoDecoder` calls
`receive`. One shared `ReceiveOutcome`, extended additively with the chain
fields (`chain_valid`, `revocation_state`, `failure_reason`,
`revocation_reason`) and a standard discriminator. `receive_openpgp` stays
untouched.

`receive_smime`: resolve the account's S/MIME private key (the p12-imported
leaf, `standard="smime"`, `key_type='cert'`, `has_private`, default-encrypt
first; the candidate loop treats "no matching recipient" as try-next, other
errors fail closed); decrypt through `SmimeBackend::decrypt`; for the nested
signed shape, verify through `verify_with_context` with the inputs resolved
from the store:

- `from_email` from the new core helper (the From↔SAN binding).
- `trust_anchor_ders` from `list_trust_anchor_certs`.
- `intermediate_ders` from `list_intermediate_certs` merged with
  `extract_intermediates` of the SignedData cert set, deduped by fingerprint.
- `crls` from the CRL cache (cache only, no fetcher; SP4c-3); a small
  list-all helper if needed, else empty (revocation unchecked).
- `signer_trust` from `get_latest_trust_decision` (Personal for the own key).

The state ladder (precise): chain invalid (missing/untrusted anchor,
expired, revoked) → Invalid; chain valid but From↔SAN mismatch → Mismatch;
chain valid + identity match + signer trust displays-as-verified
(Verified|Personal) → ValidVerified; otherwise → ValidUnverified. Harvest
the discovered signer leaf via `insert_harvested_cert` (never auto-trusted).
Write `message_crypto_results` with `chain_valid` and `revocation_state`
populated; restore the protected subject. Fail closed: no key / decrypt
failure / tamper never yields plaintext or ciphertext.

## D3: the schema and the reading integration

A migration extends `message_crypto_results`: a `standard` discriminator
column (additive) and `'stale'` in the `revocation_state` CHECK (a
table-rebuild, SQLite cannot alter a CHECK). `ResultVerdict`, `write_result`,
and the `results.rs` reader extend for the chain fields. The p12
intermediates are stored `key_type='intermediate'`, never through
`keystore.put` (which hardcodes 'cert' and would leak them into the anchor
set).

`CryptoDecoder` switches to the unified `receive`; the banner mapper learns
the S/MIME states (encrypted + verified / unverified / mismatch / invalid,
the no-key / failed closed arms reuse `closed`). `hide_crypto_parts` already
hides `smime.p7m` and the pkcs7 media types. The reading seam's structural
fail-closed clear (SP4c-1) applies unchanged. The composite decoder is
unchanged (one crypto decoder, now both kinds).

## D4: the contract

`docs/crypto-receive.md` (+ zh) gains the S/MIME section (the framing, the
verify-input resolution, the state ladder, the schema extension); the
capabilities S/MIME row's receive half lands (the note names what remains:
sign-only, the security UI, key management); a changelog fragment lands.

## Known gaps recorded, not hidden

- Sign-only S/MIME (multipart/signed detached, opaque signed-data) is
  deferred with the other detached-signature work; it is fail-open
  (cleartext + verdict), distinct from the encrypted fail-closed arms.
- CRL fetching is SP4c-3 (cache only here; revocation unchecked in
  practice). A revoked live-proof arm needs a hand-built CRL fixture.
- No key-management surface yet (SP4d): the live proof seeds the recipient
  p12 and the CA chain programmatically (the send test's pattern).
- The native signature-details dialog (failure_reason/revocation_reason
  display) is SP4d; the banner carries the reason text here.

## Test-first

Every behaviour lands with its red. The unwrap helpers (the enveloped
round-trip, the recognised-but-unextractable fail-closed arm, the
From helper); the eContent extractor (the nested signed body recovery); the
orchestrator (key resolution, the state ladder: verified / unverified /
mismatch / invalid / no-key / failed, the result-row chain fields, the
fail-closed floor); the decoder (the S/MIME banner arms). ONE gated live
harness proof: seed a CA chain (root anchor → intermediate → sender leaf)
and the recipient p12, send S/MIME encrypted+signed through the engine seam,
open the delivered row, assert the words + the chain-verified banner + the
restored subject + the result row (chain_valid, revocation_state, signer
fp) + no ciphertext; the no-key and untrusted-anchor (Invalid) and
From↔SAN-mismatch (Mismatch) negative arms; withdraw copies. No eval/pipeline
bump (the model-facing surface is unchanged).