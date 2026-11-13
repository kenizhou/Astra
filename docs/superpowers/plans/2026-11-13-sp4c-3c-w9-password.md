# SP4c-3c plan: the W9 password receive flow

Spec: docs/superpowers/specs/2026-11-13-sp4c-3c-w9-password-design.md. Branch
`sp4c-3c-w9-password` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit. A contract change:
crypto-receive.md gains the W9 password-flow note with its zh mirror in the
same commit, the capabilities OpenPGP row moves with its zh mirror, and a
changelog fragment lands. No engine change.

### Task 1: the needs-password verdict + the password orchestrator (D1)

**Files:** crates/astra-crypto-store/src/receive.rs (the additive
ReceiveState::NeedsPassword arm; after unwrap, has_skesk detects a passphrase
message and, when no PKESK key fits, the verdict is NeedsPassword; the
receive_openpgp_password(store, account, message, source, passphrase) entry
calls decrypt_symmetric with the same known_signers/extra_signer_blobs
resolution, writes the ok row + restores the subject on success, returns a
typed re-prompt signal on WrongPassphrase) + tests. The needs-password state
is transient (not a persisted row; the decrypt_state CHECK unchanged).

- [ ] Tests first (a SKESK message → NeedsPassword; the correct passphrase →
      ok + words + restored subject; WrongPassphrase → re-prompt, no
      plaintext; embedded signature verifies / unknown signer degrades); then
      the verdict + the password entry. Commit
      `feat: the OpenPGP password receive orchestrator`.
- [ ] The crypto crates are cap-exempt. The passphrase is held only in memory,
      never logged/persisted. The fail-closed arms are preserved. The existing
      receive tests stay green.

### Task 2: the decoder arm + the password prompt + the contract (D2, D3)

**Files:** crates/astra-app/src/crypto_decode.rs (the NeedsPassword banner
arm); a new password-dialog component (client/web, its own file, the
useDialogFocus idiom); a new desktop command carrying the passphrase
in-process (the crypto_wrapper precedent) + re-running the password
orchestrator over a fresh engine.message_source + refreshing the reading
snapshot (a new reading re-entry module, reading.rs near the cap); the new
wire type in the owning feature service (never ipc.ts); docs/crypto-receive.md
(the W9 note) + zh; docs/capabilities.md (the OpenPGP row) + zh;
docs/changelog/unreleased/sp4c-3c-w9-password.md (Platforms: desktop, Bump:
minor); crates/astra-app/src/tests_live_harness/crypto_receive_password.rs
(the gated live proof: hand-build a SKESK message via encrypt_symmetric,
deliver, open → needs-password; submit the passphrase → words + restored
subject; wrong passphrase → re-prompt; withdraw copies) + tests_live_harness.rs
registration.

- [ ] Tests first (the decoder banner, the command re-entry, the gated live
      proof that skips offline); then the decoder arm + dialog + command +
      docs. Commit `feat: the W9 password prompt and receive contract`.
- [ ] astra-app/client/desktop/web NOT cap-exempt: the password dialog is its
      own component, the reading re-entry a new module, the wire type in the
      feature service (never ipc.ts). The passphrase stays in-process, never
      logged/persisted. Comments present-tense; no em/en dashes; British
      English. The docs/zh mirrors move in the same commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate.
- [ ] Controller arms the live harness proof on a CLEAN harness
      (scripts/dev/harness.sh reset, then up + env; cargo test -p astra-app
      tests_live_harness::crypto_receive_password -- --test-threads=1), then
      unsets.
- [ ] Branch pushed; PR (squash expected); CI watch; squash-merge, delete
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's verdict + password orchestrator (T1); D2's decoder arm +
  prompt + D3's contract + live proof + battery + PR (T2).
- Risks: the passphrase never logged/persisted (in-memory only); the
  needs-password state is transient (the decrypt_state CHECK unchanged); the
  fail-closed arms preserved; WrongPassphrase re-prompts (no plaintext leak);
  the wire type in the feature service (ipc.ts at cap); the reading re-entry a
  new module (reading.rs near cap); the password dialog its own component.
- No eval/pipeline bump (model-facing unchanged). W9 send is itself unwired
  (the live proof hand-builds a SKESK message); wiring W9 send is a
  coordination follow-up. Live-proof lesson: reset the harness first.