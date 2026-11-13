# Junk blocking plan

Spec: docs/superpowers/specs/2026-11-14-junk-blocking-design.md. Branch
`junk-blocking` from main at the spec commit. Each task: reds first, implement,
battery, one conventional commit. One contract change: a new docs/junk-blocking.md
with its zh mirror in the same commit, the capabilities row moves with its zh
mirror, a changelog fragment lands, and porting-backlog.md moves junk blocking into
the shipped list. No engine change, no new store, no AI bump. Suppression only (the
reference red line; filing stays the Junk verb's job).

### Task 1: the blocklist preference + the suppression gate (D1)

**Files:** crates/astra-app/src/preferences/mod.rs (blocked_senders:
BTreeSet<String>, the sender_names pattern, skip_serializing_if empty);
crates/astra-app/src/protocol/intent_settings.rs (BlockSender/UnblockSender arms) +
settings/mod.rs (the set_setting setter leg, address normalisation);
crates/astra-app/src/notifications.rs (refresh the blocked set at arm, the
"sender not blocked" leg in qualifies, fail-open on a read failure, blocked beats
VIP) + tests.

- [ ] Confirm at the hook site: the sender_names preference pattern
      (preferences/mod.rs:156-158), set_setting (settings/mod.rs:147-164),
      SettingsIntent (intent_settings.rs:60), NewMailQueue arm (notifications.rs:123)
      + qualifies (:239-250), the sync settings read (sync.rs:106-111), the VIP
      normaliseAddress idiom. Reds first (the settings intent round-trip, the
      address normalisation, the queue suppresses a blocked sender's notice, an
      unblocked sender still qualifies, a failed blocklist read fails open, the
      blocked-beats-VIP order); then the preference + intent + gate. Commit
      `feat: the blocked-sender preference and suppression gate`.
- [ ] astra-app NOT cap-exempt (notifications.rs / preferences/mod.rs growth stays
      under 500; child module if needed). Newtypes; the address normalised
      lowercased/trimmed/addr-spec. The gate is the deterministic new-mail lane, NOT
      the AI incoming pass (no eval/bump). Existing tests stay green.

### Task 2: the web block surface + contract + live proof (D2, D3)

**Files:** a thin row-menu "Block sender" item (rowMenu.ts near cap, beside
vipItem); a new standalone BlockSenderDialog component (block + optional "Also
report this conversation as junk" checkbox defaulted on, reusing the shipped spam
report verb; a report failure never undoes the block) + tests; a settings
management section (list blocked senders + unblock, in an existing category); the
blocklist reads through the settings wire (no new ipc.ts type); docs/junk-blocking.md
+ zh; docs/capabilities.md row + zh; docs/changelog/unreleased/junk-blocking.md
(Platforms: desktop, Bump: minor); docs/porting-backlog.md + zh;
crates/astra-app/src/tests_live_harness/junk_blocking_proof.rs (block a sender,
deliver from them via the harness submission path, assert the queue holds NO
notice; unblock, deliver again, assert the notice returns; withdraw copies) +
tests_live_harness.rs registration.

- [ ] Reds first (the block dialog blocks + the optional report leg, a report
      failure never undoes the block, the management section unblocks; the
      offline-skipping gated live proof); then the dialog + menu + settings + docs.
      Commit `feat: the junk blocking surface and contract`.
- [ ] client/web not exempt: the dialog is its own component (rowMenu.ts near cap,
      keep the menu item thin); ipc.ts at cap (the field rides the existing settings
      snapshot, no new type). vitest traps: act() mid-test, mock the service whole.
      Comments present tense; no em/en dashes; British English; source files end
      with a newline (CI rustfmt stricter). docs/zh mirrors same commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix sidecar
      test; npx tsc --noEmit && npx vitest run (client/web); cargo run -p xtask
      -- gate. If the plugin tests fail with AlreadyGranted, clean the stale
      /tmp/astra-desktop-plugin-* scratch dirs (Windows pid-recycle flake) and
      re-run. WATCH DISK SPACE (clean the cargo incremental cache if it balloons).
- [ ] Controller arms the live proof on a clean harness (scripts/dev/harness.sh
      reset, then up + env; cargo test -p astra-app
      tests_live_harness::junk_blocking_proof -- --test-threads=1), then unset.
- [ ] Push the branch; PR (squash expected); CI watch; squash merge, delete the
      branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's preference + gate (T1); D2's web surface + D3's contract +
  live proof + battery + PR (T2).
- Risks: the suppression-only red line (NO auto-filing; filing stays the Junk
      verb's job, a named non-goal); the gate is the deterministic lane not the AI
      pass (no eval/bump); fail-open on a blocklist read failure; blocked beats VIP;
      rowMenu.ts near cap (thin item + own dialog); ipc.ts at cap (settings wire).
      Device-local blocklist, no CLI verb, settings category detail (recorded gaps).
- No eval/pipeline bump (model-facing unchanged). No reference deviation for the
      minimal suppression scope. Live-proof lesson: reset the harness first; clean
      stale plugin scratch dirs if the battery flakes; ensure trailing newlines.