# SP4c-1 plan: OpenPGP receive through the reading pipeline

Spec: docs/superpowers/specs/2026-11-13-sp4c-openpgp-receive-design.md.
Branch `sp4c-openpgp-receive` from main at the docs commit. Each task: reds
first, the implementation, the battery, a conventional commit. A contract
change: a new docs/crypto-receive.md (+ zh), the capabilities OpenPGP row
moves with its zh mirror, and a changelog fragment lands. No engine change.

### Task 1: the receive orchestrator (D1)

**Files:** crates/astra-crypto-store/src/receive.rs (new, beside wrap.rs,
cap-exempt: detect+unwrap PGP/MIME multipart/encrypted, resolve the account
private key via SqliteKeyStore, decrypt_with_signers directly with keystore
+ collected signers, map DecryptOutcome weak/unverifiable to the signature
state, write message_crypto_results with the Rust-side PK linkage, restore
the protected subject; a neutral engine-free result type) + receive_tests.rs;
a new unwrap helper (the inverse of pgp_mime::wrap_encrypted, reusing
mime::split_message) in astra-crypto-core if it belongs there, else in the
store crate.

- [ ] Tests first (unwrap, key resolution, the state mapping, the result-row
      linkage, no-key/unknown-signer/tampered fail-closed); then the
      orchestrator. Commit `feat: the OpenPGP receive orchestrator`.
- [ ] The store crate is cap-exempt; astra-crypto-core is cap-exempt. No
      engine change. Fail closed: no key / failure never yields plaintext.

### Task 2: the reading integration (D2)

**Files:** crates/astra-app/src/crypto_decode.rs (new sibling, reading.rs is
near the cap: a built-in MessageDecoder calling the orchestrator, mapping to
DecodeOutcome html/plain/banner, the decrypted legs through render_html_leg,
the fail-closed banner states, hide the ciphertext pseudo-attachment) +
crypto_decode_tests.rs (the reading_decode_tests.rs fake pattern);
client/desktop/src/boot.rs + a composite decoder (the built-in crypto
alongside PluginDecoder, crypto verdict precedence for recognised messages);
crates/astra-app/src/tests_live_harness/crypto_receive.rs (the gated live
proof: seed astra2 key, send encrypted astra1->astra2, open the row, assert
the snapshot words + verified banner + restored subject + result row, the
no-key negative arm, withdraw copies).

- [ ] Tests first (the offline integration reds + the gated live proof that
      skips offline); then the adapter + composite + proof. Commit
      `feat: OpenPGP receive through the reading pipeline`.
- [ ] Cap watch: astra-app/client NOT cap-exempt — new modules, not growth
      of reading.rs (462) or ipc.ts (at cap; no new wire type needed, the
      banner rides the existing SecurityBannerView). The live proof skips
      without ASTRA_LIVE_HARNESS (the controller arms it).

### Task 3: the contract + close-out (D3)

**Files:** docs/crypto-receive.md (the receive path, the fail-closed states,
the detection boundary, the result-row linkage, the protected-subject
restore) + docs/zh/crypto-receive.zh-CN.md; docs/capabilities.md (the
OpenPGP row's receive half lands; the note names what remains) + zh;
docs/changelog/unreleased/sp4c-openpgp-receive.md (Platforms: desktop, Bump:
minor).

- [ ] The docs + zh + row + fragment. Commit
      `docs: the OpenPGP receive contract`.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate.
- [ ] Controller arms the live harness proof (scripts/dev/harness.sh up +
      env; cargo test -p astra-app tests_live_harness::crypto_receive --
      --test-threads=1), then unsets.
- [ ] Branch pushed; PR (squash expected); CI watch; squash-merge, delete
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's orchestrator (T1); D2's reading integration + live
  proof (T2); D3's contract + row + fragment + battery + armed proof + PR
  (T3).
- Risks: FAIL CLOSED is load-bearing (no key / failure / tamper never
  renders ciphertext; the plugin fail-open does NOT apply to crypto
  verdicts); decrypt_with_signers directly (the trait drops the weak/
  unverifiable channels); the composite decoder precedence (crypto vs
  plugin); the protected-subject restore; the result-row PK linkage; the
  sanitise pipeline re-runs on the decrypted legs; no engine change; the
  500-cap bites in astra-app/client (new modules).
- No eval/pipeline bump (model-facing surface unchanged). Deferred: inline
  armor, detached signatures, S/MIME (SP4c-2), Autocrypt/CRL/W9 (SP4c-3),
  drafts at rest (SP4c-4), key management + token registry + UI (SP4d).