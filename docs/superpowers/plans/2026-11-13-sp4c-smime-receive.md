# SP4c-2 plan: S/MIME receive through the reading pipeline

Spec: docs/superpowers/specs/2026-11-13-sp4c-smime-receive-design.md. Branch
`sp4c-smime-receive` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit. A contract change:
crypto-receive.md gains the S/MIME section with its zh mirror, the
capabilities S/MIME row moves with its zh mirror, and a changelog fragment
lands. No engine change. Scoped to ENCRYPTED S/MIME (sign-only deferred).

### Task 1: the helpers, the orchestrator, the schema (D1, D2, D3 schema)

**Files:** crates/astra-crypto-core/src/smime_mime.rs (new, mirror pgp_mime:
is_smime_enveloped / unwrap_enveloped for application/pkcs7-mime +
x-pkcs7-mime smime-type=enveloped-data, base64 to DER, the recognised-but-
unextractable fail-closed arm) + a From-extraction helper in mime.rs (pub);
crates/astra-crypto-smime/src/ (a pub extract_signed_content eContent
extractor + a SignedData sniff, CMS parsing stays here);
crates/astra-crypto-store/src/receive_smime.rs (new, mirror receive.rs:
resolve the S/MIME private leaf, decrypt, verify_with_context with the store
inputs [from_email, anchors, intermediates merged+deduped, CRL cache, signer
trust], the state ladder, harvest the signer leaf, write the result row with
chain fields, restore the subject, fail closed) + receive_smime_tests.rs;
a unified receive dispatcher in receive.rs (sniff PGP/MIME vs S/MIME, route;
receive_openpgp untouched); ReceiveOutcome/ResultVerdict/write_result/
results.rs extended additively for the chain fields + standard; a migration
(standard column + 'stale' in the revocation_state CHECK, table-rebuild).

- [ ] Tests first (unwrap round-trip + fail-closed arm, From helper, eContent
      extraction, the orchestrator state ladder + result-row chain fields +
      fail-closed floor); then the helpers, orchestrator, dispatcher, schema.
      Commit `feat: the S/MIME receive orchestrator`.
- [ ] The crypto crates are cap-exempt. No engine change. The state ladder is
      precise: untrusted/missing anchor or revoked -> Invalid; From/SAN
      mismatch -> Mismatch; chain+identity+not-trusted -> ValidUnverified;
      +Verified/Personal -> ValidVerified. p12 intermediates stored
      key_type='intermediate', never via keystore.put.

### Task 2: the reading integration + live proof (D3 integration)

**Files:** crates/astra-app/src/crypto_decode.rs (switch to the unified
receive; the S/MIME banner arms: encrypted+verified/unverified/mismatch/
invalid, the closed arms reuse; smime.p7s hiding if needed) +
crypto_decode_tests.rs (the S/MIME banner reds);
crates/astra-app/src/tests_live_harness/crypto_receive_smime.rs (the gated
live proof: seed a CA chain [root anchor -> intermediate -> sender leaf] +
the recipient p12, send S/MIME encrypted+signed through the engine seam,
open the row, assert words + chain-verified banner + restored subject +
result row [chain_valid, revocation_state, signer fp] + no ciphertext; the
no-key / untrusted-anchor Invalid / From-SAN Mismatch negative arms; withdraw
copies); astra-app Cargo.toml dev-dep astra-crypto-smime features=["testing"];
tests_live_harness.rs registration.

- [ ] Tests first (the offline banner reds + the gated live proof that skips
      offline); then the decoder dispatch + banner + proof. Commit
      `feat: S/MIME receive through the reading pipeline`.
- [ ] Cap watch: astra-app/client NOT cap-exempt (crypto_decode.rs stays
      <500; the live proof <500). The reading seam's structural fail-closed
      clear (SP4c-1) applies unchanged; the composite decoder unchanged. The
      live proof skips without ASTRA_LIVE_HARNESS (the controller arms it).

### Task 3: the contract + close-out (D4)

**Files:** docs/crypto-receive.md (the S/MIME section: the framing, the
verify-input resolution, the state ladder, the schema extension) + zh;
docs/capabilities.md (the S/MIME row's receive half lands; the note names
what remains) + zh; docs/changelog/unreleased/sp4c-smime-receive.md
(Platforms: desktop, Bump: minor).

- [ ] The docs + zh + row + fragment. Commit
      `docs: the S/MIME receive contract`.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate.
- [ ] Controller arms the live harness proof (scripts/dev/harness.sh up +
      env; cargo test -p astra-app tests_live_harness::crypto_receive_smime
      -- --test-threads=1), then unsets.
- [ ] Branch pushed; PR (squash expected); CI watch; squash-merge, delete
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's unwrap + D2's orchestrator + D3's schema (T1); D3's
  integration + the live proof (T2); D4's contract + row + fragment + battery
  + armed proof + PR (T3).
- Risks: the state ladder precision (Invalid vs ValidUnverified vs Mismatch
  vs ValidVerified); fail closed (no key/failure/tamper never renders
  ciphertext); the nested eContent extraction; the From/SAN binding; the
  schema CHECK rebuild; p12 intermediates never via put (trust overreach);
  the 500-cap in astra-app/client (new modules); no engine change.
- No eval/pipeline bump (model-facing unchanged). Deferred: sign-only S/MIME,
  CRL fetching (SP4c-3), key management + the signature-details UI (SP4d).