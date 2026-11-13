# SP4c-3a plan: Autocrypt consume

Spec: docs/superpowers/specs/2026-11-13-sp4c-3a-autocrypt-consume-design.md.
Branch `sp4c-3a-autocrypt-consume` from main at the docs commit. Each task:
reds first, the implementation, the battery, a conventional commit. A
contract change: crypto-receive.md gains the Autocrypt-consume note with its
zh mirror in the same commit, the capabilities OpenPGP row moves with its zh
mirror, and a changelog fragment lands. No engine change. Cap-exempt crypto
crates only (no network, no UI, no migration).

### Task 1: the inbound parser + receive hooks (D1, D2)

**Files:** crates/astra-crypto-core/src/mime.rs (a tiny pub header-value
accessor, the from_email precedent); crates/astra-crypto-store/src/
autocrypt.rs (the consume parser beside the emitters: unfold + read the
Autocrypt / Autocrypt-Gossip values, split attributes, extract addr /
prefer-encrypt / keydata, base64-decode, parse_first_cert, the addr binding,
stage_collected_key_if_absent source='autocrypt') + autocrypt_consume_tests.rs;
crates/astra-crypto-store/src/receive.rs (the cleartext Autocrypt hook on the
outer headers + the gossip hook after unwrap_protected_headers).

- [ ] Tests first (the emit→consume round-trip, addr≠From discard, malformed
      keydata stages nothing, the gossip parse, the re-open dedup, the
      receive hooks stage the keys); then the parser + hooks. Commit
      `feat: consume inbound Autocrypt keys into the collected staging`.
- [ ] The crypto crates are cap-exempt. No network, no UI, no migration. The
      fail-closed receive arms are preserved (staging is silent, never
      changes the decrypt verdict). The existing receive tests stay green.

### Task 2: the contract + live proof + close-out (D3)

**Files:** docs/crypto-receive.md (the Autocrypt-consume note) + zh;
docs/capabilities.md (the OpenPGP row's note names the consume half) + zh;
docs/changelog/unreleased/sp4c-3a-autocrypt-consume.md (Platforms: desktop,
Bump: minor); crates/astra-app/src/tests_live_harness/crypto_receive_autocrypt.rs
(the gated live proof: the send gate emits the Autocrypt header, the
recipient opens, assert the collected_keys row; the gossip arm; the addr≠From
and malformed negatives; withdraw copies) + tests_live_harness.rs registration.

- [ ] The docs + zh + row + fragment + the gated live proof (skips offline).
      Commit `docs: the Autocrypt consume contract`.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate.
- [ ] Controller arms the live harness proof on a CLEAN harness
      (scripts/dev/harness.sh reset, then up + env; cargo test -p astra-app
      tests_live_harness::crypto_receive_autocrypt -- --test-threads=1), then
      unsets.
- [ ] Branch pushed; PR (squash expected); CI watch; squash-merge, delete
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's parser + D2's hooks (T1); D3's contract + row +
  fragment + live proof + battery + PR (T2).
- Risks: the addr binding (Autocrypt addr must match From, else discard);
  malformed keydata stages nothing (never panics); the re-open dedup (the
  (account, standard, fingerprint, source) key); staging is silent and never
  changes the decrypt verdict; the gossip headers live inside the protected-
  headers wrapper; the pub core helper respects the detection boundary.
- No eval/pipeline bump (model-facing unchanged). No network/UI/migration.
  The review/promote UI is SP4d; CRL (3b) and W9 (3c) are separate slices.
- Live-proof lesson (from SP4c-2): reset the harness before a clean proof;
  seed leaves the send gate can chain (signed directly by the anchored root).