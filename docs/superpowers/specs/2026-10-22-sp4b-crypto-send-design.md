# SP4b design: the crypto store and the send gate (Astra-side outbox)

**Status.** Decided 2026-10-22. Branch `sp4b-crypto-integration` from main.
User decisions 2026-10-22: fully port the kylins crypto, frontend and
backend; adopt the **Astra-side bypass path** for the send gate (no changes
to the upstream engine), with MIME assembly in Astra before the wrap; and
keep sensitive drafts encrypted at rest (their note: "how to handle the Draft
encrypted emails? we need to keep the sensitive draft email in encrypted
also").

**Why the bypass exists.** The pinned engine's submit path takes a `Draft`
and assembles MIME internally at drain; there is no raw-MIME op and no
hook (checked the pinned rev and upstream master). kylins' `apply_crypto`
wraps assembled MIME, so the crypto gate cannot ride the engine's queue.
The engine is not to be touched. So crypto sends travel an Astra-side
path: Astra assembles the message with the engine's own
`engine_rfc5322::assemble_message` (in-graph, byte-identical output), wraps
it with the kylins gate, and queues the wrapped bytes in an Astra-side
durable crypto outbox drained by a small Astra-side submitter. Non-crypto
sends stay on the engine's queue untouched. The cost is a second, much
smaller send engine for crypto mail only; the honest bounds of that are
recorded in Known gaps.

**Scope.** The crypto store (schema + KeyStore + trust + resolution), the
send gate (apply_crypto semantics verbatim), the crypto outbox and its
transport (SMTP submit + Sent-copy filing), and the account's crypto-method
setting that turns the gate on per account. Encrypted drafts at rest are
the read half (SP4c) with the decode pipeline; the token registry and
native UI are SP4d.

## D1: crates/astra-crypto-store, rusqlite, one crypto.sqlite

A new crate beside astra-crypto-*: a rusqlite store at `crypto.sqlite`
beside the AI store, with the kylins schema verbatim: `crypto_keys` (with
`origin` incl. 'harvested', CHECKs, indexes), `trust_decisions` (append-only,
latest-wins), `collected_keys`, `contact_pinned_keys`, `crl_cache`,
`message_crypto_results`, `send_as_aliases`. The store implements
`astra_crypto_core::KeyStore` (SqliteKeyStore port from kylins'
`platform/keystore.rs`, account-bound, `standard|fingerprint` KeyId
encoding, the OpenPGP subkey-fingerprint fallback), the trust-decision CRUD
and signer trust resolution, the recipient resolution
(`resolve_recipient_status_at` port: NoKey with the collected hint, Expired
first, own key Ready, decision ladder Verified/Personal/Unverified ->
 Ready, Rejected, Undecided, AcceptedOtherEmail), `pin_preferred_handle`,
`recipient_trust_failures`, and the default-signing-key row (get/set).
sqlx becomes rusqlite (Astra's store idiom, matching astra-ai); the schema
SQL ports byte-for-byte.

## D2: the send gate, apply_crypto verbatim

`apply_crypto` ported at `crates/astra-crypto-core/apply.rs` (its home in
the core crate, since the store is its only new dependency and the core
already names it): passthrough when neither sign nor encrypt; `NoSigningKey`
when sign is set with no default key; encrypt = sender first
(encrypt-to-self) + To/Cc/Bcc, every recipient must resolve `Ready` or the
typed `RecipientTrust` failure stands (no ciphertext, no plaintext
fallback); pin-aware handle selection so gate and encrypt use the same key;
S/MIME-only recipient-cert chain validation against the account's
`key_type='cert'` anchors (`InvalidRecipientCert`); W4 protected-headers
subject hiding (default ON, fail-safe to ON on read error); PGP
Autocrypt-Gossip injection (sign+encrypt only); `EncryptOp.sign_with`
composition (never clear-sign-then-encrypt); per-standard wrapping:
`application/pkcs7-mime` enveloped / `multipart/signed` for S/MIME, RFC
3156 `multipart/encrypted` / `multipart/signed` for PGP (kylins' pgp_mime
module ported with it). The W9 password mode (SKESK symmetric) comes with
its own arm: `PasswordConflict` on key-encrypt + password, `PasswordRequired`
on empty, no recipient resolution, no gossip, protected headers identical.

## D3: the Astra-side crypto outbox and its submitter

At submit (astra-app's `submit_*` arms, in the gate seam), when the
account's crypto method is `openpgp` or `smime`: Astra assembles the MIME
with `engine_rfc5322::assemble_message`, applies the gate, and enqueues
the wrapped bytes in `crypto_outbox` (a table in crypto.sqlite: id,
account_id, standard, wire_bytes, filed_bytes, state queued/sending/
unconfirmed/failed, error, created/attempted). The crypto submitter
(`crates/astra-crypto-store/src/transport.rs`) is deliberately small and
explicit about what it is: EHLO, AUTH PLAIN, MAIL FROM, RCPT TO, DATA with
dot-stuffing, RSET on error, over the account's dialled SMTP settings and
the engine-tls connector; then the Sent-copy filing by IMAP APPEND of the
filed bytes (with the Bcc header, the assembler's filed variant). A lost
connection mid-DATA lands the row in `unconfirmed` (the engine's
needs-confirmation shape); the user answers delivered (settle) or not
(re-send), mirroring the engine outbox's contract instead of duplicating
its code. Rows surface through the existing Sending snapshot as extra
rows (same panel, no second outbox UI), with send-now and withdraw verbs
routing to the crypto queue for its rows.

## D4: the account's crypto method and the gate switch

Each account gains a crypto-method setting (`none | openpgp | smime`), in
the account settings snapshot and editable on the Accounts page, persisted
beside the account config. `none` is the zero-cost default: no assembly,
no store touch. The composer gains the per-draft sign/encrypt flags (the
SendDraft shape's `sign`/`encrypt` booleans) as composer options in the
compose intents, defaulting from the account's policy. A draft saved while
crypto is on is the SP4c encrypted-draft problem; this slice only gates
sends.

## D5: the proofs

Unit reds pin: the store schema and KeyStore round trips (put/get/
find_by_email/remove, subkey fallback), trust decisions latest-wins, the
recipient resolution ladder (each state, expiry first, pin-preferred),
`recipient_trust_failures`; the gate (passthrough, NoSigningKey,
recipient-trust refusal never reaches encrypt, chain invalid recipient,
protected headers on, password conflict/required, sign-then-encrypt
composition shape) over the MemoryKeyStore fixtures kylins uses; the
outbox (queue/drain/retry/withdraw/unconfirmed/settle) with a scripted
SMTP fixture; and the harness smoke: generate a key, astra1 -> astra2
encrypted send through the crypto outbox, the Sent copy filed, a
missing-key recipient blocked with the held row standing.

## Known gaps recorded, not hidden

- The bypass is a second, smaller send engine for crypto mail: it owns
  retry/backoff, unconfirmed semantics, and transport robustness the
  engine's submitter spent releases hardening (partial-DATA handling,
  AUTH renewal, deadline discipline). The subset is honest and tested, and
  the gap is named here so it is never mistaken for parity.
- IMAP/Graph/Google/JMAP providers are untouched: the crypto submitter is
  SMTP+IMAP(APPEND) only this slice; accounts without IMAP SMTP settings
  answer a typed unavailable error, never a silent drop.
- Token-backed keys route through the backend's TokenProvider when a
  provider is registered (SP4d's registry); a token key with no provider
  fails typed (`TokenNotPresent`), never falls back to software.
- CRL fetching rides the receive side (SP4c); the S/MIME send-side chain
  gate uses stored anchors only, as kylins' apply does.
- Encrypted drafts at rest (the user's note) are SP4c with the decode
  pipeline, since both are decrypt-at-rest problems; the draft pool's
  shape is unchanged by this slice.
