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
- **Needs password** (a SKESK slot and no PKESK key fits): a "passphrase
  required" banner beside the password prompt affordance, no plaintext, no
  result row (a transient interaction, not a persisted state). The W9 password
  flow below decrypts it with a user-supplied passphrase.
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

## The Autocrypt consume path (SP4c-3a)

`receive_openpgp` consumes the inbound Autocrypt keys into the collected
staging area, silent and after the verdict. Two header shapes feed it:

- The cleartext `Autocrypt:` header on the outer header block carries the
  sender's key. The consume unfolds the value, splits the attributes on `;`,
  extracts `addr`, `prefer-encrypt` and `keydata`, base64-decodes the keydata
  (the binary minimal export) and parses the first cert for its fingerprint.
  Autocrypt Level 1 binds the `addr` to the From address (case-insensitive);
  a mismatch discards the header.
- The `Autocrypt-Gossip:` headers ride inside the encrypted part, in the
  decrypted inner entity's root header block (the protected-headers wrapper
  the send gate emits). A gossip `addr` is the recipient peer, with no From
  binding; each peer key stages.

Staging is silent: `stage_collected_key_if_absent` writes a `collected_keys`
row with `source='autocrypt'`, deduped on (account, standard, fingerprint,
source) so a re-opened message never duplicates. Malformed keydata stages
nothing and never panics. The consume runs after the verdict is computed and
persisted, so it never changes this message's decrypt or signature outcome;
the fail-closed arms return before it, so a failed receive stages nothing.

The payoff is immediate: `receive_openpgp` already offers the collected certs
to `decrypt_with_signers` and `resolve_signer`, so a staged key improves the
next message's embedded-signature verification. The review/promote surface for
collected keys landed in SP4d-2 (the review surface below); staging here is
silent.

## The CRL fetching path (SP4c-3b)

The S/MIME chain check consumes the CRL cache; SP4c-3b populates it. Before
`verify_with_context`, the receive hook extracts the CRL distribution-point
URLs from the signer leaf and every intermediate (`crl_distribution_point_urls`
in the smime crate, the SP4c-2 parsing rule), deduplicates them, and
fetches-or-caches each through the `CrlFetcher` port. Only `http` and `https`
URLs are admissible: a distribution-point URL is attacker-influenced (it comes
from the cert), so any other scheme is dropped.

The port follows the `SendCryptoPolicy` precedent: the store defines
`CrlFetcher`, the host implements it, so the store never depends on astra-app.
The desktop implementation (`HttpCrlFetcher`) does a bounded HTTP GET: a short
timeout, a response-size cap, http(s) only, and no redirects. A fresh cached
CRL stands without a fetch; a stale or missing one triggers a fetch and an
upsert. The fetch is fail-soft: any transport, scheme, size, or parse failure
degrades the cert to unchecked revocation and NEVER blocks the message open
(the chain already soft-fails). The chain validates as-of signing time, so a
freshly fetched CRL is still checked against the signing instant. A forged CRL
from a malicious URL fails the chain's CRL-signature check (the checker
verifies against the issuing CA).

The revocation states are now reachable in practice. A CRL that revokes the
signer's serial hard-fails the chain: `chain_valid=false`,
`revocation_state=revoked`, the Invalid signature state, and the distinguished
revoked banner. With no usable CRL (an empty cache, a failed fetch, or a stale
CRL the chain reads as unchecked) the open proceeds with
`revocation_state=unchecked`. The offline no-op fetcher keeps the reading path
cache-only when no host fetcher is attached.

Two considerations are recorded rather than hidden. Privacy: fetching a CRL
reveals to the CA which certs are being validated (a tracking vector); an
OCSP/stapling alternative is a follow-up. Residual blind SSRF: the scheme
filter admits http(s) but does not block private or loopback addresses, so a
malicious distribution point can still aim the bounded GET at an internal
host; address-range filtering is a hardening follow-up. The fetch stays bounded
and inline; a background prefetch is a follow-up.

## The W9 password flow (SP4c-3c)

A passphrase-protected OpenPGP message carries a SKESK slot (symmetric
encryption) rather than, or beside, a PKESK key-wrap. After unwrap,
`has_skesk` detects it. When no PKESK key fits, `receive_openpgp` answers the
transient `NeedsPassword` verdict instead of no-key/failed: a message may carry
BOTH slots, and the key path still wins when a key fits, so the SKESK flag only
redirects the no-fitting-key arms. A hard error (tamper, bad signature) fails
closed even when a SKESK slot is present, never downgraded to a prompt.

The decoder maps `NeedsPassword` to a fail-closed "passphrase required" banner
beside the password prompt affordance (`ReadingSnapshot.password_prompt`): no
legs, no ciphertext, no result row. The reading pane mounts its password dialog
on the affordance. The dialog collects the passphrase and submits it through a
desktop command that carries it in-process (the `crypto_wrapper` precedent:
secrets stay in host memory, never logged or persisted). The command re-runs
`receive_openpgp_password` over a fresh `engine.message_source` and refreshes
the reading snapshot through the identical render seam (the reading re-entry
module, split from `reading.rs` near the cap).

`receive_openpgp_password` decrypts through `decrypt_symmetric` with the SAME
`known_signers` / `extra_signer_blobs` resolution as the key path, so an
embedded signature still verifies against known peers and an unknown signer
degrades, never aborts. On success it lands the identical verdict, restored
subject, and `ok` row as the key path, and the affordance drops (the dialog
closes over the words). A wrong passphrase answers the transient `WrongPassword`
re-prompt signal: the dialog re-prompts, naming the refusal, no plaintext, no
row. A structural failure fails closed exactly as the key path. A cancel never
reaches the orchestrator: the verdict stays needs-password (fail closed).

The passphrase is held only in memory: it moves into the backend as a
`SecretBox` (zeroized on drop) and is never logged or persisted. The
needs-password and wrong-password states are transient interactions, not
persisted rows (the `decrypt_state` CHECK stays ok/no-key/failed/n/a); success
writes ok. A re-open re-prompts (no cached passphrase).

## The drafts-at-rest mirror (SP4c-4)

The engine persists compose drafts as plaintext (the outbox payload while
queued, the synced blob cache and message_body/FTS after warm) and cannot be
changed. Astra builds the Draft itself before `engine.put_draft`, so the body
is interceptable. SP4c-4 adds an encrypted draft MIRROR held by Astra plus a
resume read seam: the durable Astra-held copy is vault-encrypted and resume
prefers it, falling back to the engine body only for a draft that was never
mirrored.

The mirror is the `encrypted_drafts` table (migration 0005), keyed by
(account_id, draft_key) where the draft_key is the draft's stable Message-ID
(`Draft.message_id`, pre-generated, no angle brackets), the same identity the
resume derives from the stored message. The body is the vault blob
(`hex(0x01 || nonce || ct+tag)`, AES-256-GCM under the OS-keyring master key),
wrapped with the AAD `astra:{account}:draft:{message_id}:1` mirroring the
private-key AAD, so a blob lifted from one account or draft fails AEAD
verification anywhere else. The store treats the payload as opaque bytes; the
Draft to JSON serde lives in astra-app (`drafts/encrypt.rs`).

The save hook runs BEFORE `engine.put_draft`: it serialises the rendered Draft
and writes the mirror. A vault error refuses the save (fail closed, no
plaintext persisted; the composer keeps the draft in memory and the refusal is
logged). The account row is registered idempotently first (the FK target, the
MIME-wrapper idiom), so a plain account with no crypto wrapper still saves.
The mirror survives a save the engine KEEPS: a parked save (the outbox records
the op before the provider call) and an applied-but-unrecorded write both keep
the draft, so the mirror stands. It rolls back only when the engine kept
nothing (a payload-encode failure before the op is enqueued); an ambiguous
store error keeps the mirror conservatively, since an orphan mirror is harmless
ciphertext while rolling back a kept draft's mirror would strip its at-rest
protection. A re-save upserts the same key; the digest/supersede logic is
untouched. The delete hook fires on both discard paths and the post-send
cleanup (which routes through the composition discard).

The resume read seam calls `get_encrypted_draft` first. The three outcomes are
distinct: a present mirror deserialises the Draft and maps it to DraftResume
(the words come from the vault, not the engine cache); an absent mirror
(`Ok(None)`, the draft was never mirrored) falls back to `engine.message_body`
as before; a mirror that exists but cannot be decrypted fails closed (a
"cannot decrypt" banner on the reading snapshot, no resume, NEVER the plaintext
fallback). The DraftResume and reading snapshot shapes are unchanged.

The plaintext lives only in memory and inside the vault blob: the master key
stays in the OS keyring (zeroized, never logged), and nothing here logs the
draft's content.

Recorded gap: the engine's own at-rest copies remain plaintext (the outbox
payload while queued, the synced blob cache, message_body/FTS after warm). This
slice protects the durable Astra-held copy and routes resume through the vault;
it does not purge the engine residue. Encrypt-before-engine (hand put_draft a
ciphertext-bearing Draft) is the follow-up if "never plaintext on disk" is the
requirement; it makes server-side drafts unreadable to other clients and
degrades search/snippets, so it is a product decision, not this slice. The AI
draft_pool.reply plaintext is adjacent at-rest exposure, out of scope (same
vault, a follow-up). CLI parity: the CLI reads engine bodies directly; the
mirror is a desktop reading-path protection, like the other receive surfaces.

## The key import surface (SP4d-1)

The receive and send arcs stage and consume keys, but a person needs a way to
bring a key in. SP4d-1 lands the desktop key import surface: import your own
private/identity key (so received encrypted mail decrypts) and import a peer's
public certificate (so signatures verify and encrypted sends resolve). The CLI
key verbs and the sidecar virtual tools are SP4d-1b; the review/promote surface
for collected keys landed in SP4d-2 (the review surface below).

Two paths, deliberately separate to avoid the has_private pollution:

- The public-cert path (`astra-crypto-store::import_public_cert`) parses the
  material (OpenPGP armor-or-binary via `parse_certs`, or an S/MIME PEM/DER
  cert via `fingerprint_of_cert_der` / `cert_summary_of_der`) and stores the
  public-only blob through `upsert_crypto_key` with `private_data: None`,
  `key_type: 'public'` (OpenPGP) or `'cert'` (S/MIME), `origin: 'imported'`. It
  never routes through the backend `import_key` / keystore `put`, which record
  a private blob and would set `has_private` true, polluting the receive
  decryption-candidate filter and the resolver's Personal/Ready branch.
  Secret-bearing OpenPGP input is refused here: a private key belongs to the
  private path. A cross-standard import is refused (`import_public_cert_as`
  pins the standard).
- The private/identity path reuses the existing backend `import_key`
  (passphrase-gated secret decrypt, persist through the keystore), which
  correctly records the private blob wrapped at rest under the account-bound
  AAD. The S/MIME arm uses `import_key_with_chain`, landing the bag's
  intermediates as `intermediate` rows (never trust anchors).

The app method (`astra-app/src/keys.rs`, the `reading_password.rs` shape)
sniffs the material and dispatches: an OpenPGP TSK or an S/MIME key bundle
takes the private path, everything else the public path. The first private key
of a standard becomes the account's default signing key when none stands, so
the account can sign straight after the import; a later import never steals
the standing default. `list_keys` answers the account's rows across both
standards, public view only.

The desktop command (`commands/crypto.rs`) carries the bytes and the optional
passphrase in-process (the `submit_reading_password` precedent: secrets stay
in host memory, never logged or persisted). The web affordance lives in the
settings Keys category (`features/keys`): a file picker and a paste field, an
optional passphrase held only in local state and cleared the moment it is
spent. The wire types ride `services/crypto.ts`, never `ipc.ts`.

A shared trust-write helper (`record_import_trust`) records a `verified` /
`unverified` decision over `put_trust_decision`; the collected-key promote
surface (SP4d-2) reuses it. The explicit decision is what makes a peer key
send-usable. The public import itself records the default acceptance: an
imported peer key lands `unverified` (the send resolver treats `unverified` as
encryption-qualified, so the import is send-usable straight away; a later
out-of-band verification promotes it to `verified`). A standing decision is
never downgraded, so a re-import of a verified key keeps `verified`; a cert
with no email user id carries no peer address to decide against and the review
surface (SP4d-2) owns it.

A public re-import of a fingerprint already held privately preserves the
standing row: the default-sign / default-encrypt flags, the token binding, and
the `key_type` (a private key is never demoted to `public`). The upsert's
conflict clause overwrites those fields unconditionally, so the import carries
the standing values forward; a later import never steals the standing default
on either path.

The passphrase is held only in memory: it moves through as a `SecretBox`
(zeroized on drop) and is never logged or persisted; only the vault-wrapped
key blob lands at rest.

The gated live proof (`tests_live_harness/crypto_key_import.rs`) imports both
keys through the product surface (no programmatic seeding), sends an
encrypted+signed mail, and asserts it decrypts and verifies, pinning the
has_private guard on the public imports. It skips unless `ASTRA_LIVE_HARNESS=1`.

## The collected-key review surface (SP4d-2)

The receive and send arcs stage peer keys silently into `collected_keys` (the
Autocrypt consume above; gossip, WKD, and the S/MIME harvest likewise), but a
person needs a way to review, accept, or reject a staged key. SP4d-2 lands the
desktop review/promote surface over the complete store layer: list the staged
keys, accept (promote into `crypto_keys` plus a trust decision), accept as
verified, or reject (remove the staging row plus a rejected decision). The CLI
key verbs and the sidecar virtual tools are SP4d-1b (the desktop-first
precedent).

The app methods (`astra-app/src/collected.rs`, the `keys.rs` shape) expose:

- `list_collected(account)`: the account's staged rows across both standards,
  newest first (the store's `seen_at DESC, id DESC` ordering merged across the
  standards, a same-second tie breaking on the row id). Public material only.
- `promote_collected(account, standard, fingerprint, email, verified)`: the
  store `promote_collected_key` (which drains the staging rows and never
  downgrades a standing keystore row) THEN the trust write. This pairing is
  load-bearing: promote alone is only half an acceptance, the explicit decision
  is what makes the promoted key send-usable (the resolver answers Undecided
  without one) and lifts the S/MIME harvested-anchor exclusion. A `verified`
  choice always records verified (an upgrade); an `unverified` choice records
  unverified only when no decision stands (never downgrades a standing
  verified/personal decision). The acceptance binds `email`, or the staged
  row's peer email when none is named; a key with no peer email carries no
  address to decide against and the promote still lands the key.
- `reject_collected(account, id, standard, fingerprint, email)`: removes the
  staging row by id and records a `rejected` decision. Staging is silent and
  decision-blind, so a re-opened source message re-stages the key and the review
  list offers it again; the rejection stands: an unverified promote of the
  re-staged copy never downgrades it, and the resolver answers `Rejected` once a
  keystore row exists (a staged-only key answers `NoKey`, decisions are read only
  against a keystore row). Decision-aware staging/listing (never re-offer a
  rejected key) is a recorded follow-up, not this slice.

A shared store helper (`record_promote_trust` in `import.rs`) records the
promote acceptance with the no-downgrade guard, reusing `record_import_trust`
over `put_trust_decision`.

The desktop commands (`commands/crypto.rs`: `list_collected`,
`promote_collected`, `reject_collected`) carry the account, standard,
fingerprint, staging id, email, and verified flag, registered in the host. No
secret crosses this surface (collected keys are public material). The web
affordance lives in the settings Keys category
(`features/keys/CollectedKeysReview.tsx`, joined to the Keys detail): each
staged row shows the fingerprint, peer email, source, and standard, and offers
Accept (unverified), Accept as verified, and Reject. The fingerprint is shown
so a person can verify it out of band before choosing Accept as verified. The
wire types ride `services/crypto.ts`, never `ipc.ts`.

The gated live proof (`tests_live_harness/crypto_collected_review.rs`) receives
an Autocrypt message that stages a peer key, lists it through the product
surface, promotes it (the trust decision lands), and sends that peer encrypted
mail admitted by the promote alone (no programmatic trust seeding); it rejects
a second staged key and asserts it is removed, then withdraws every copy. It
skips unless `ASTRA_LIVE_HARNESS=1`.

## Known gaps, recorded rather than left silent

- Encrypted receive only, per standard: OpenPGP PGP/MIME (SP4c-1) and S/MIME
  enveloped-data (SP4c-2). Inline PGP armor and the detached-signature shapes
  (PGP multipart/signed, S/MIME multipart/signed and opaque signed-data) are
  follow-ups; Autocrypt consume landed in SP4c-3a (the receive half above);
  CRL fetching landed in SP4c-3b (the fetching path above); the W9 password
  flow landed in SP4c-3c (the password flow above); encrypted drafts at rest
  landed in SP4c-4 (the mirror above).
- The W9 SEND side is itself unwired (`apply_password_crypto` has no callers,
  `SendDraft` has no `password_protected` field): the live proof hand-builds a
  SKESK message via `encrypt_symmetric`. Wiring W9 send is a coordination
  follow-up. The passphrase is never persisted, so a re-open re-prompts; a
  session passphrase cache is a follow-up. The password dialog is shell-only
  (the reading pane), like the other receive surfaces.
- Autocrypt staging is silent: the review/promote UI for collected keys landed
  in SP4d-2 (the review surface above). The send gate emits `Autocrypt-Gossip`
  inside the ciphertext but no cleartext own `Autocrypt` header yet; the consume
  reads a cleartext header when an incoming message carries one (a peer
  client's), and the own-header emission is a follow-up. Gossip consume reads
  the protected-headers wrapper the send gate emits; third-party gossip
  placements beyond that wrapper are a follow-up.
- S/MIME revocation fetching landed in SP4c-3b (the CRL fetching path above):
  the receive hook fetches-or-caches the signer chain's distribution-point CRLs
  through the fail-soft `CrlFetcher` port, so a revoked signer hard-fails the
  chain in practice. A fetch failure or an empty cache still reads unchecked
  (fail-soft, never blocks the open). The chain's Stale revocation state
  persists as unchecked (the Good/Stale granularity is a follow-up). The
  residual blind-SSRF hardening (private/loopback address filtering) and the
  privacy-preserving OCSP/stapling alternative remain follow-ups.
- The key import surface landed in SP4d-1 (the key import surface above): own
  private key and peer public cert import through the product surface, and the
  live proof now imports rather than seeds. The CLI key verbs and the sidecar
  virtual tools are SP4d-1b; the review/promote surface for collected keys
  landed in SP4d-2 (the review surface above). Key generation, export,
  default-key selection beyond the first import, and the composer
  recipient-trust UI are follow-ups.
- The collected-key review surface landed in SP4d-2 (the review surface above):
  list, accept, accept-as-verified, and reject through the product surface, the
  promote paired with the trust decision that makes the key send-usable, and
  the live proof now promotes rather than seeds. The CLI key verbs (`keys
  collected`/`promote`) and the sidecar virtual tools are SP4d-1b (the verb
  lands with its virtual permission tool, needs-approval default). A pin
  affordance (`set_pinned_key`, marking a promoted key preferred) is a
  follow-up; the promote lands the key send-usable without it. The composer
  recipient-trust UI (showing the trust state at send time) is a follow-up.
  Staging and the review list are decision-blind: a rejected key re-stages when
  its source message is re-opened and the list offers it again. The rejection
  still stands (an unverified promote never downgrades it; the resolver answers
  Rejected once a keystore row exists), so this is a re-offer, never a silent
  re-acceptance. Decision-aware staging/listing (never re-offer a rejected key)
  is a follow-up behaviour change.
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
| Autocrypt consume: inbound parse, addr binding, silent staging | ✅ | `astra-crypto-store/src/autocrypt.rs`, `receive.rs` |
| CRL fetching: DP extraction, the fail-soft fetcher port, the receive hook | ✅ | `astra-crypto-smime/src/cert.rs`, `astra-crypto-store/src/crl.rs`, `receive_smime.rs`, `client/desktop/src/crl_fetch.rs` |
| The W9 password flow: SKESK detection, needs-password verdict, in-memory passphrase, re-prompt | ✅ | `astra-crypto-store/src/receive.rs`, `astra-app/src/reading_password.rs`, `crypto_decode.rs`, `client/desktop/src/commands/crypto.rs`, `client/web/src/components/reading-pane/PasswordDialog.tsx` |
| Drafts at rest: the encrypted mirror, the save/discard hooks, the resume read seam, fail-closed refusal | ✅ | `astra-crypto-store/src/drafts.rs`, `migrations/0005_encrypted_drafts.sql`, `astra-app/src/drafts/encrypt.rs`, `drafts.rs`, `drafts/resume.rs` |
| Key import: the public-only store path, the private backend path, the has_private guard, the in-memory passphrase, the trust write | ✅ | `astra-crypto-store/src/import.rs`, `astra-app/src/keys.rs`, `client/desktop/src/commands/crypto.rs`, `client/web/src/features/keys/KeysDetail.tsx` |
| Collected-key review: list, the promote-plus-trust pairing, the no-downgrade guard, the rejected decision | ✅ | `astra-crypto-store/src/import.rs` (`record_promote_trust`), `astra-app/src/collected.rs`, `client/desktop/src/commands/crypto.rs`, `client/web/src/features/keys/CollectedKeysReview.tsx` |
| The live proofs (send encrypted, open, decrypt, the negative arms, drafts at rest, key import, collected review) | ✅ | `tests_live_harness/crypto_receive.rs`, `crypto_receive_smime.rs`, `crypto_receive_autocrypt.rs`, `crypto_receive_crl.rs`, `crypto_receive_password.rs`, `crypto_drafts_at_rest.rs`, `crypto_key_import.rs`, `crypto_collected_review.rs` |
| Inline armor, detached signatures | N/A | SP4c-3, the gaps above |
| Key generation and export, the token registry, the native security UI | N/A | SP4d follow-ups |