# SP4d-1 plan: the key import core

Spec: docs/superpowers/specs/2026-11-13-sp4d-1-key-import-design.md. Branch
`sp4d-1-key-import` from main at the spec commit. Each task: reds first,
implement, battery, one conventional commit. One contract change:
crypto-receive.md gains the key-import note with its zh mirror in the same
commit, the capabilities row moves with its zh mirror, and a changelog fragment
lands. No engine change. CLI key verbs + sidecar virtual tools are SP4d-1b.

### Task 1: the store import functions (D1)

**Files:** a new crates/astra-crypto-store/src/import.rs (import_public_cert
over astra-crypto-openpgp parse_certs + fingerprint_of + cert_to_public_blob,
public-only upsert_crypto_key key_type:'public' origin:'imported'
private_data:None; S/MIME parity via fingerprint_of_cert_der /
cert_summary_of_der key_type:'cert'; a shared trust-write helper over
trust.rs put_trust_decision); store.rs module wiring + tests. The private path
reuses the existing backend import_key (no reinvention).

- [ ] Reds first (import_public_cert round-trips a public cert with has_private
      false; a malformed input stages nothing and never panics; the
      fingerprint/user-id are extracted; the public path never sets a private
      blob; a cross-standard import is rejected; the trust-write helper records
      a decision); then the import function + helper. Commit
      `feat: the public-cert import store path`.
- [ ] Crypto crate exempt from the cap. MUST NOT route through backend
      import_key / put_sync (the has_private pollution). Existing store tests
      stay green.

### Task 2: the app method + desktop command + web affordance + contract (D2, D3)

**Files:** a new crates/astra-app/src/keys.rs child module (impl App block:
import_key dispatching public vs private by secret material, list_keys; the
reading_password.rs shape, lib.rs at the cap no growth); a new desktop command
in client/desktop/src/commands/crypto.rs (in-process bytes/passphrase, the
submit_reading_password precedent) registered in client/desktop/src/lib.rs; a
new client/web/src/features/keys/ import component (file/paste) + wire types in
services/crypto.ts (never ipc.ts) joined to the settings rail;
docs/crypto-receive.md (the key-import note) + zh; docs/capabilities.md (the
keys row) + zh; docs/changelog/unreleased/sp4d-1-key-import.md (Platforms:
desktop, Bump: minor); crates/astra-app/src/tests_live_harness/
crypto_key_import.rs (gated live proof: import a peer public cert + own private
key through the product surface, receive an encrypted+signed message decrypts
and verifies with no programmatic seeding; withdraw copies) +
tests_live_harness.rs registration.

- [ ] Confirm at the hook site: import_key dispatches public (no secret) vs
      private (secret + passphrase) correctly; the private path reuses backend
      import_key; list_keys returns the account rows. Reds first (import
      dispatches public vs private; list returns the rows; the passphrase never
      leaks; the offline-skipping gated live proof); then the app method +
      command + web affordance + docs. Commit
      `feat: the key import surface and contract`.
- [ ] astra-app/client/desktop/web not exempt: new core logic in the new child
      module (lib.rs/commands.rs at the cap, no growth; a new mod line in
      commands.rs would breach the cap, declare the command child from
      crypto.rs); wire types in services/crypto.ts (never ipc.ts). The
      passphrase stays in memory, zeroized, never logged/persisted. Comments
      present tense; no em/en dashes; British English. docs/zh mirrors same
      commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix sidecar
      test; npx tsc --noEmit && npx vitest run (client/web); cargo run -p xtask
      -- gate. If the plugin tests fail with AlreadyGranted, clean the stale
      /tmp/astra-desktop-plugin-* scratch dirs (Windows pid-recycle flake) and
      re-run.
- [ ] Controller arms the live proof on a clean harness (scripts/dev/harness.sh
      reset, then up + env; cargo test -p astra-app
      tests_live_harness::crypto_key_import -- --test-threads=1), then unset.
- [ ] Push the branch; PR (squash expected); CI watch; squash merge, delete the
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's import functions + trust helper (T1); D2's app method +
  command + web affordance + D3's contract + live proof + battery + PR (T2).
- Risks: the has_private pollution (the public path must use upsert_crypto_key
  private_data:None, never backend import_key/put_sync); the private path reuses
  the existing backend import_key; the passphrase never leaks; lib.rs/commands.rs
  at the cap (new child modules, no growth); ipc.ts at the cap (wire types in
  crypto.ts). The trust-write helper is reused by SP4d-2's promote.
- No eval/pipeline bump (model-facing unchanged). CLI key verbs + sidecar virtual
  tools are SP4d-1b. Live-proof lesson: reset the harness first; clean stale
  plugin scratch dirs if the battery flakes on AlreadyGranted.