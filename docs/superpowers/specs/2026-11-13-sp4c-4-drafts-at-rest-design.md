# SP4c-4 design: encrypted drafts at rest

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** Compose drafts are persisted by the engine as plaintext: the outbox
pending_op_payload JSON while queued, then the synced blob cache and
message_body/FTS after warm. Astra builds the Draft itself before handing it to
engine.put_draft, so the body is interceptable, but Engine::open offers no
store/key injection point and the MimeWrapper seam does not cover put_draft.
With no engine change allowed, the engine's own copies cannot be encrypted in
place. This slice adds an encrypted draft mirror held by Astra plus a resume
read seam: the durable Astra-held copy is vault-encrypted and resume prefers
it, falling back to the engine body only for drafts that were never mirrored.
The vault (AES-256-GCM, an OS-keyring master key, vault.rs) already ships. No
engine change; no eval/pipeline bump.

## D1: the encrypted draft mirror (store)

A new migration 0005 in astra-crypto-store adds encrypted_drafts keyed by
(account_id, draft_key), body_enc BLOB holding the vault blob, updated_at,
PRIMARY KEY (account_id, draft_key). The draft_key is the draft's stable
Message-ID header (Draft.message_id, pre-generated, no angle brackets), the
same identity resume derives from the engine message. The blob is wrapped with
a new AAD namespace astra:{account}:draft:{message_id}:1 mirroring the
private-key AAD (astra:{account}:private_key:1), so cross-account/field replay
fails AEAD. The keyring master key is the root; no new master-key table.

The store treats the payload as opaque bytes (it never names engine_api::Draft,
so no new dependency): put_encrypted_draft(account, key, plaintext) vault-
encrypts with the draft AAD and upserts; get_encrypted_draft(account, key)
reads the row (no row -> Ok(None)) then vault-decrypts (Ok(Some(bytes)) or a
typed error); delete_encrypted_draft(account, key) removes the row. Keyring-
unavailable is the typed CryptoVaultError::Keyring; the stack's discipline is
fail-closed.

## D2: the save/discard hooks (core)

A new child module drafts/encrypt.rs in astra-app (drafts.rs is near the cap)
serialises the rendered Draft to JSON (Draft derives Serialize + Deserialize)
and drives the mirror. The write hook in save_draft_gated runs BEFORE
engine.put_draft: encrypt + put the mirror first, and on a vault error refuse
the save (fail closed, no plaintext persisted; the composer keeps the draft in
memory and surfaces the error). On a successful put_draft the mirror stands; if
put_draft itself fails the mirror is rolled back. The digest/supersede logic is
unaffected (a re-save upserts the same key). The delete hook fires wherever the
draft is removed (both discard paths and the post-send cleanup). The
CryptoStore is always installed (boot.rs), so the mirror applies to every
draft; the vault master key is lazily created in the OS keyring on first use.

## D3: the resume read seam + contract

resume_latest_draft / resume_draft_by_key call get_encrypted_draft first. The
three outcomes are distinct: Ok(Some(bytes)) -> deserialise the Draft and map
it to DraftResume (the words come from the vault, not the engine cache);
Ok(None) -> the draft was never mirrored (written elsewhere), fall back to
engine.message_body as today; Err(vault) -> the mirror exists but cannot be
decrypted (keyring gone), a fail-closed banner, never the plaintext fallback.
docs/crypto-receive.md gains the drafts-at-rest note (the mirror, the read
seam, the in-memory plaintext, the fail-closed refusal) and mirrors it to zh.
The capabilities row names the drafts-at-rest landing. A changelog fragment
lands.

## Known gaps recorded, not hidden

- The engine's own at-rest copies remain plaintext: the outbox payload while
  queued, the synced blob cache, and message_body/FTS after warm. This slice
  protects the durable Astra-held copy and routes resume through the vault; it
  does not purge the engine residue. Encrypt-before-engine (hand put_draft a
  ciphertext-bearing Draft) is the follow-up if "never plaintext on disk" is
  the requirement; it makes server-side drafts unreadable to other clients and
  degrades search/snippets, so it is a product decision, not this slice.
- A keyring-unavailable save is refused (fail closed), surfaced as an error;
  the draft is not persisted plaintext. This is the stack's discipline.
- The AI draft_pool.reply plaintext is adjacent at-rest exposure, out of scope
  here (same vault, a follow-up).
- CLI parity: the CLI reads engine bodies directly today; the mirror is a
  desktop reading-path protection, like the other receive surfaces.

## Test-first

Every behaviour lands with its red. Store tests (put/get/delete round-trip
through the vault; the AAD binds account + draft key so a cross-account blob
fails; a missing row is Ok(None); keyring-unavailable is typed). Core tests
(the save hook writes the mirror before put_draft and refuses on a vault error;
discard deletes it; resume prefers the mirror, falls back on Ok(None), fail-
closed on Err). ONE gated live harness proof: with the test master key armed,
save a draft, assert the mirror row is ciphertext (not the plaintext), resume
returns the words, discard removes the mirror; withdraw copies. No live model
proof; the offline battery is the proof.