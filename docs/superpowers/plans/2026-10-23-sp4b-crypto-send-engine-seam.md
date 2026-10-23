# SP4b-seam plan: the crypto gate moves into the engine

Spec: docs/superpowers/specs/2026-10-23-sp4b-crypto-send-engine-seam-design.md.
Branch `engine-crypto-seam` from main. Engine delta first so the tree
always builds; reds first where behaviour changes; conventional commits;
one squash PR at the end (not a vendor-sync PR, so squash is fine).

### Task 1: the engine seam (D1, D2)

**Files (engine):** `engine-provider/src/draft_attachment.rs` (the split),
`submit.rs` (flags + re-exports), `mime_wrap.rs` (new), `lib.rs`;
`engine-api/src/lib.rs` (re-exports);
`provider-imap/src/{config.rs, account.rs, provider.rs}` (the threading),
`wrap.rs` (new: `PreparedMessage` + `prepare_message` + `filed_bytes`),
`filing.rs` (async hoisted `prepare`, `Submission.filed`, `file_result`,
`refile`); engine `docs/agent-guidance/{imap-smtp,providers,store-and-sync}.md`.

- [ ] Engine tests first (Draft serde back-compat, scripted wrapper:
  wrap only when flagged, wire vs filed differ only in Bcc, permanent
  refusal pre-dial on all three transports, refile re-wraps); engine
  battery + its gate + stalwart-live. Commit `engine: add the MIME-wrap
  seam for outbound crypto`.

### Task 2: the Astra wrapper and the bypass removal (D3, D5)

**Files (store):** `src/wrap.rs` (new, with `wrap_tests.rs`),
`src/migrations/0003_drop_crypto_outbox.sql`, `src/lib.rs`, `src/store.rs`,
`Cargo.toml`; deleted: `src/outbox.rs`, `src/transport.rs`, their tests.

**Files (app):** `crypto.rs` (gutted to `CryptoFlags`),
`crypto_outbox.rs` (deleted), `lib.rs`, `submit.rs` (flag stamp),
`outbox.rs` (crypto routing removed), `crypto_tests.rs` (rewritten),
`settings/accounts.rs` + fakes + settings_proof (trait signature).

- [ ] Store tests first (strip_bcc, gate_failure_words, address_spec,
  factory None/always-refusing, end-to-end wrap, permanent refusal
  carrying the trust words); app seam tests. Commit
  `feat: wrap sends in the engine, retire the bypass`.

### Task 3: the host installation (D4)

**Files:** `client/desktop/src/crypto_wrapper.rs` (new, replaces
`crypto_send.rs` + tests), `boot.rs` (store before dial, AppState field),
`boot/stored.rs` (dial_stored + DialConnector), `commands.rs`,
`headless.rs`, `src/ai/live_*/harness.rs`, `account_ops.rs` (re-dial +
non-IMAP refusal).

- [ ] Desktop tests (wrapper resolution; store_crypto_method re-dial and
  the fail-closed refusal). Commit `feat: install the crypto wrapper at dial`.

### Task 4: the harness smoke and the record (D6)

**Files:** `tests_live_harness/crypto_send.rs` (rewritten to the engine
path: delivered sealed, Sent filed by the engine, recipient decrypts,
missing-key recipient stands Failed with the trust reason, nothing on the
server); docs: `docs/plugins.md` Track B, `docs/capabilities.md` rows,
`docs/changelog/unreleased/sp4b-crypto-send.md` (amended),
`docs/sending.md`/`docs/settings.md` (checked), `docs/upstream-sync.md`
(delta narrative), each with its zh mirror.

- [ ] Harness serial green; full battery, clippy, xtask gate, engine
  battery green; commit `feat: prove the engine-seamed crypto send live
  and record it`; push, PR, squash merge.

## Self-review notes

- Spec coverage: D1+D2 land in Task 1, D3+D5 in Task 2, D4 in Task 3, D6
  in Task 4; the removal rides Tasks 2-3 so the tree builds at every step.
- Interface chain: the trait (T1) is what the store implements (T2) and
  the host installs (T3); the live proof closes the loop (T4).
- Risks: the flag stamp must never default to false on a crypto account;
  `refile()` must re-wrap; the split precedes every added engine line;
  the engine delta is exactly what `git diff vendor-sync main --
  vendor/engine` shows at the end.
