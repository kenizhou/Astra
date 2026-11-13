# SP4c-3b plan: CRL fetching

Spec: docs/superpowers/specs/2026-11-13-sp4c-3b-crl-fetching-design.md. Branch
`sp4c-3b-crl-fetching` from main at the docs commit. Each task: reds first,
the implementation, the battery, a conventional commit. A contract change:
crypto-receive.md gains the CRL-fetching note with its zh mirror in the same
commit, the capabilities S/MIME row moves with its zh mirror, and a changelog
fragment lands. No engine change.

### Task 1: the DP extraction + the fetcher port + the receive hook (D1, D2)

**Files:** crates/astra-crypto-smime/src/cert.rs (the CRL distribution-point
URL extraction beside cert_summary_of_der; x509-parser crl_distribution_points
→ GeneralName::URI; http(s) only) + tests; crates/astra-crypto-store/src/crl.rs
(the CrlFetcher port + the fetch-or-cache logic: fresh cache used without a
fetch, stale/missing triggers fetch + upsert, fetch failure degrades to
unchecked) + tests with a fake fetcher; crates/astra-crypto-store/src/
receive_smime.rs (the hook before verify_with_context: extract DP URLs from
leaf + intermediates, fetch-or-cache, list_crls feeds the chain);
client/desktop/src/ (the host CrlFetcher impl: reqwest bounded GET, short
timeout, size cap, http(s) only) + wiring.

- [ ] Tests first (the DP extraction, the fetch-or-cache with a fake fetcher,
      the fetch-failure fail-soft, the receive hook revoked/unchecked); then
      the extraction + port + hook + host impl. Commit
      `feat: fetch CRLs for the S/MIME receive revocation check`.
- [ ] The crypto crates are cap-exempt; client/desktop NOT (keep the host impl
      cohesive). The fetch is fail-soft (never blocks the open), bounded
      (timeout + size cap), http(s) only. The store never depends on astra-app.
      The existing receive tests stay green.

### Task 2: the contract + live proof + close-out (D3)

**Files:** docs/crypto-receive.md (the CRL-fetching note; update the "cache
empty, revocation unchecked" gap) + zh; docs/capabilities.md (the S/MIME row
names the revocation checking) + zh; docs/changelog/unreleased/
sp4c-3b-crl-fetching.md (Platforms: desktop, Bump: minor);
crates/astra-crypto-smime/src/testing.rs (a CRL builder; the CA builders
already carry cRLSign); crates/astra-app/src/tests_live_harness/
crypto_receive_crl.rs (the gated live proof: a local HTTP test server serves
the hand-built CRL; seed the CA chain, revoke the sender leaf serial, open the
mail, assert revocation_state=revoked + chain_valid=false + the revoked banner;
the fetch-failure negative → unchecked; withdraw copies) + tests_live_harness.rs
registration.

- [ ] The docs + zh + row + fragment + the CRL builder + the gated live proof
      (skips offline). Commit `docs: the CRL fetching contract`.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate.
- [ ] Controller arms the live harness proof on a CLEAN harness
      (scripts/dev/harness.sh reset, then up + env; cargo test -p astra-app
      tests_live_harness::crypto_receive_crl -- --test-threads=1), then unsets.
- [ ] Branch pushed; PR (squash expected); CI watch; squash-merge, delete
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's extraction + D2's port/hook (T1); D3's contract + row +
  fragment + CRL builder + live proof + battery + PR (T2).
- Risks: the DP URL is untrusted (http(s) only, timeout, size cap); the fetch
  is fail-soft (never blocks the open); a forged CRL fails the chain's
  CRL-signature check; the store never depends on astra-app (the port); the
  chain validates as-of signing time; the privacy consideration recorded.
- No eval/pipeline bump (model-facing unchanged). The W9 password flow (3c) is
  a separate slice. Live-proof lesson: reset the harness before a clean proof.