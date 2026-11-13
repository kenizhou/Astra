# SP4c-4 plan: encrypted drafts at rest

Spec: docs/superpowers/specs/2026-11-13-sp4c-4-drafts-at-rest-design.md. Branch
`sp4c-4-drafts-at-rest` from main at the spec commit. Each task: reds first,
implement, battery, one conventional commit. One contract change:
crypto-receive.md gains the drafts-at-rest note with its zh mirror in the same
commit, the capabilities row moves with its zh mirror, and a changelog fragment
lands. No engine change.

### Task 1: the encrypted draft mirror (D1)

**Files:** crates/astra-crypto-store/src/migrations/0005_encrypted_drafts.sql
(new; encrypted_drafts(account_id REFERENCES accounts, draft_key, body_enc
BLOB, updated_at, PRIMARY KEY(account_id, draft_key))); a new
crates/astra-crypto-store/src/drafts.rs (put_encrypted_draft /
get_encrypted_draft / delete_encrypted_draft over vault.rs, AAD
astra:{account}:draft:{key}:1 mirroring keys.rs private_aad; opaque bytes, the
store never names engine_api::Draft); store.rs migration chain + module wiring
+ tests.

- [ ] Reds first (put/get/delete round-trip through the vault; the AAD binds
      account + draft key so a cross-account blob fails AEAD; a missing row is
      Ok(None); keyring-unavailable is the typed CryptoVaultError::Keyring);
      then the migration + the store methods. Commit
      `feat: the encrypted draft mirror`.
- [ ] Crypto crate exempt from the cap. The master key stays in the OS keyring
      (vault.rs), zeroized, never logged. Existing store tests stay green.

### Task 2: the save/discard hooks + the resume seam + contract (D2, D3)

**Files:** a new crates/astra-app/src/drafts/encrypt.rs (Draft <-> JSON, the
mirror driver; drafts.rs near the cap); save_draft_gated write hook (before
engine.put_draft, fail-closed refusal on a vault error, rollback if put_draft
fails) + the discard/post-send delete hooks (drafts.rs); the resume read seam
(drafts/resume.rs: Ok(Some) -> the vault words, Ok(None) -> the engine-body
fallback, Err -> a fail-closed banner); docs/crypto-receive.md (the
drafts-at-rest note) + zh; docs/capabilities.md (the drafts row) + zh;
docs/changelog/unreleased/sp4c-4-drafts-at-rest.md (Platforms: desktop, Bump:
minor); crates/astra-app/src/tests_live_harness/crypto_drafts_at_rest.rs (gated
live proof: the test master key armed, save -> the mirror row is ciphertext,
resume -> the words, discard -> the mirror gone; withdraw copies) +
tests_live_harness.rs registration.

- [ ] Confirm at the hook site: the stable draft key is Draft.message_id and
      resume derives the same Message-ID from the engine message; the rendered
      Draft is the payload (Serialize + Deserialize both ways). Reds first
      (the save hook writes the mirror before put_draft and refuses on a vault
      error; discard deletes it; resume prefers the mirror, falls back on
      Ok(None), fail-closed on Err; the offline-skipping gated live proof);
      then the hooks + the seam + the docs. Commit
      `feat: the drafts-at-rest vault seam and contract`.
- [ ] astra-app/client/desktop/web not exempt: new core logic in the new child
      module, never drafts.rs/reading.rs/lib.rs/commands.rs growth; no new wire
      type in ipc.ts (the mirror is core-side, the resume snapshot is unchanged
      in shape). Comments present tense; no em/en dashes; British English.
      docs/zh mirrors in the same commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix sidecar
      test; npx tsc --noEmit && npx vitest run (client/web); cargo run -p xtask
      -- gate.
- [ ] Controller arms the live proof on a clean harness (scripts/dev/harness.sh
      reset, then up + env; cargo test -p astra-app
      tests_live_harness::crypto_drafts_at_rest -- --test-threads=1), then
      unset.
- [ ] Push the branch; PR (squash expected); CI watch; squash merge, delete the
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's mirror (T1); D2's save/discard hooks + D3's resume seam +
  contract + live proof + battery + PR (T2).
- Risks: the engine residue stays plaintext (recorded gap, encrypt-before-
  engine is the follow-up); fail-closed save refusal on keyring-unavailable;
  the absent-vs-undecryptable distinction at resume (Ok(None) falls back, Err
  never does); the draft key must agree across save/resume/discard; new core
  logic in a child module (cap). The vault master key never leaves the keyring
  / memory.
- No eval/pipeline bump (model-facing unchanged). Live-proof lesson: reset the
  harness first; arm the test master key (the testing feature) so the vault
  works without a real OS keyring.