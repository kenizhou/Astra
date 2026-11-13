# SP4d-1b plan: the CLI key verbs and sidecar virtual tools

Spec: docs/superpowers/specs/2026-11-13-sp4d-1b-cli-key-verbs-design.md. Branch
`sp4d-1b-cli-key-verbs` from main at the spec commit. Each task: reds first,
implement, battery, one conventional commit. One contract change: docs/cli.md
gains the verb rows + gating map + counts with its zh mirror in the same commit,
the capabilities row moves with its zh mirror, and a changelog fragment lands. No
engine change. Public material only; private import stays desktop.

### Task 1: the five CLI verbs (D1)

**Files:** cli/Cargo.toml (the astra-crypto-store path dep + testing dev-feature);
a new cli/src/verbs/keys.rs (the five verb bodies + the lazy crypto.sqlite open +
the promote/reject orchestration replicated from astra-app/src/collected.rs);
cli/src/verbs/keys_tests.rs; the scaffolding answered in verb.rs (variants +
name() + mutating()), parse.rs (arms + SUBVERBS), mod.rs (the two execute arms +
module registration), audit.rs (arg_preview arms), completions.rs (descriptor +
PROBES).

- [ ] Confirm at the hook site: account_id resolution (the AccountId newtype),
      crypto.sqlite path (data_dir.join, the 5s busy timeout), the store function
      signatures (import_public_cert/_as, list_crypto_keys_for_account,
      list_collected_keys_for_account, promote_collected_key + record_promote_trust,
      remove_collected_key + put_trust_decision). Reds first (drive run() with a
      temp data dir + a seeded crypto.sqlite via register_account + the test
      master key: list/collected answer the shaped rows; import lands a public
      cert + the unverified trust; promote pairs the trust write; reject removes +
      records rejected; preview writes nothing; the audit row per invocation;
      secret-bearing input refused pointing at the desktop; the FK-unregistered
      error; --preview on a read verb is a usage error); then the verbs +
      scaffolding. Commit `feat: the CLI key verbs`.
- [ ] cli is NOT cap-exempt: mod.rs (468) and parse.rs (457) have ~30-40 lines of
      headroom for five verbs — keep the execute arms one-liners and the crypto
      open in keys.rs; if a file breaches, split a child module (the E0119 trap:
      a trait impl cannot split). The promote/reject orchestration is spelled
      twice (CLI + app) by design, pinned by tests on both sides (docs/cli.md).
      arg_preview is shapes only, never caller text. Existing CLI tests stay green.

### Task 2: the sidecar tools + host gating + contract (D2, D3)

**Files:** a new sidecar/src/tools/gated.ts (extract the shared runGated / execCli
/ buildCliArgv / defineMailTool from mailTools.ts, freeing its headroom); a new
sidecar/src/tools/keyTools.ts (the five tools, createKeyTools); sidecar/src/index.ts
(wire createKeyTools into both harnesses); sidecar/src/tools/keyTools.test.ts +
keyToolsMap.test.ts (the stub-cli pattern, a keys stub copy); the host
client/desktop/src/agent/mod.rs default_policy (+5 rows: reads auto, writes
confirm) + permissions.rs shaped_arg_preview (+5 arms) + policy_tests.rs (+5 rows)
+ permissions_tests.rs (the shapes); docs/cli.md (verb table + gating map + counts
ten->fifteen tools) + zh; docs/capabilities.md (the row) + zh;
docs/changelog/unreleased/sp4d-1b-cli-key-verbs.md (Platforms: desktop, Bump:
minor).

- [ ] Reds first (the sidecar tools pin the exact argv grammar + the approval
      envelope level/args + approve/reject/abort + the map pin; the host policy
      pins the five rows + the preview shapes); then the extraction + tools +
      gating + docs. Commit `feat: the key virtual tools and contract`.
- [ ] sidecar is NOT cap-exempt: mailTools.ts (487) cannot host the tools — the
      gated.ts extraction is required; mailTools.test.ts (497) and index.test.ts
      (487) are near the cap, keep new tests in sibling files. The three pins
      (policy_tests, permissions_tests, the sidecar map test) move together.
      confirm_preview stays a recorded gap (ship confirm). No secret crosses the
      surface; the passphrase is never a CLI flag (private import stays desktop).
      Comments present tense; no em/en dashes; British English. docs/zh mirrors
      same commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix sidecar
      test; npx tsc --noEmit && npx vitest run (client/web); cargo run -p xtask
      -- gate. If the plugin tests fail with AlreadyGranted, clean the stale
      /tmp/astra-desktop-plugin-* scratch dirs (Windows pid-recycle flake) and
      re-run.
- [ ] The offline battery is the proof (the import/promote/reject mechanisms are
      already live-proven through the product surface in SP4d-1 / SP4d-2; a
      CLI-level live proof is new ground, a follow-up).
- [ ] Push the branch; PR (squash expected); CI watch; squash merge, delete the
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's five CLI verbs (T1); D2's sidecar tools + host gating + D3's
  contract + battery + PR (T2).
- Risks: the tight caps (cli mod.rs/parse.rs ~30-40 headroom, sidecar mailTools.ts
  at cap -> gated.ts extraction); the promote/reject orchestration spelled twice
  (pin both); the FK-unregistered edge; the three pins move together; private
  import blocked (keyring rule, recorded gap); confirm_preview stays a gap. No
  secret crosses the surface; arg_preview shapes only.
- No eval/pipeline bump (model-facing unchanged). The offline battery is the
  proof. Live-proof lesson: clean stale plugin scratch dirs if the battery flakes
  on AlreadyGranted.