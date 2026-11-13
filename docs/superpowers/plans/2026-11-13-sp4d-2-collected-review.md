# SP4d-2 plan: the collected-key review and promote surface

Spec: docs/superpowers/specs/2026-11-13-sp4d-2-collected-review-design.md. Branch
`sp4d-2-collected-review` from main at the spec commit. Each task: reds first,
implement, battery, one conventional commit. One contract change:
crypto-receive.md gains the review/promote note with its zh mirror in the same
commit, the capabilities row moves with its zh mirror, and a changelog fragment
lands. No engine change. CLI key verbs + sidecar virtual tools are SP4d-1b.

### Task 1: the app review methods + the promote trust write (D1)

**Files:** a new crates/astra-app/src/collected.rs child module (impl App:
list_collected, promote_collected, reject_collected; the reading_password.rs /
keys.rs shape, lib.rs at the cap no growth); a shared store helper in
crates/astra-crypto-store/src/import.rs (record the promote acceptance with the
no-downgrade guard, reusing record_import_trust); store.rs/lib.rs wiring if
needed + tests.

- [ ] Confirm at the hook site: promote_collected_key(store, account, standard,
      fingerprint, email) drains staging + never downgrades a standing keystore
      row; list_collected_keys_for_account returns the staged rows newest first;
      remove_collected_key takes the staging id (i64). Reds first (list returns
      the staged rows; promote lands the key AND records the trust decision so it
      is send-usable; a verified promote upgrades, an unverified promote never
      downgrades a standing verified decision; reject removes the staging row and
      records rejected; a no-email promote still lands the key); then the app
      methods + the store helper. Commit
      `feat: the collected-key review and promote methods`.
- [ ] Crypto crate exempt from the cap; astra-app not (new child module, no
      lib.rs growth). The promote-plus-trust pairing is load-bearing: promote
      alone leaves the resolver Undecided and the send gate aborts. Existing
      tests stay green.

### Task 2: the desktop commands + the web review UI + contract (D2, D3)

**Files:** new desktop commands in client/desktop/src/commands/crypto.rs (or a
child declared from it; commands.rs at the cap, no new mod line there) registered
in client/desktop/src/lib.rs; a new client/web/src/features/keys/ review
component (list + fingerprint/peer/source/standard display + Accept / Accept as
verified / Reject) joined to the Keys detail; wire types in services/crypto.ts
(never ipc.ts); docs/crypto-receive.md (the review/promote note) + zh;
docs/capabilities.md (the review/promote row) + zh;
docs/changelog/unreleased/sp4d-2-collected-review.md (Platforms: desktop, Bump:
minor); crates/astra-app/src/tests_live_harness/crypto_collected_review.rs (gated
live proof: receive an Autocrypt message staging a peer key, list through the
product surface, promote + trust decision, send that peer encrypted mail admitted
by the promote alone; reject a second staged key, assert removed; withdraw copies)
+ tests_live_harness.rs registration.

- [ ] Reds first (the commands carry the fields; the web review lists/accepts/
      rejects; the offline-skipping gated live proof); then the commands + web UI
      + docs. Commit `feat: the collected-key review surface and contract`.
- [ ] astra-app/client/desktop/web not exempt: new core logic in the child module
      (lib.rs/commands.rs at the cap, no growth); wire types in services/crypto.ts
      (never ipc.ts). No secret crosses this surface (collected keys are public);
      the fingerprint is shown for out-of-band verification. Comments present
      tense; no em/en dashes; British English. docs/zh mirrors same commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix sidecar
      test; npx tsc --noEmit && npx vitest run (client/web); cargo run -p xtask
      -- gate. If the plugin tests fail with AlreadyGranted, clean the stale
      /tmp/astra-desktop-plugin-* scratch dirs (Windows pid-recycle flake) and
      re-run.
- [ ] Controller arms the live proof on a clean harness (scripts/dev/harness.sh
      reset, then up + env; cargo test -p astra-app
      tests_live_harness::crypto_collected_review -- --test-threads=1), then
      unset.
- [ ] Push the branch; PR (squash expected); CI watch; squash merge, delete the
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's app methods + promote trust write (T1); D2's commands + web
  review UI + D3's contract + live proof + battery + PR (T2).
- Risks: the promote-plus-trust pairing (promote alone is half an acceptance, the
  SP4d-1 lesson); the no-downgrade guard (an unverified promote never downgrades a
  standing verified decision); reject records rejected so a re-staged copy is
  refused; lib.rs/commands.rs at the cap (new child modules); ipc.ts at the cap
  (wire types in crypto.ts). No secret crosses the review surface.
- No eval/pipeline bump (model-facing unchanged). CLI key verbs + sidecar virtual
  tools are SP4d-1b. Live-proof lesson: reset the harness first; clean stale
  plugin scratch dirs if the battery flakes on AlreadyGranted.