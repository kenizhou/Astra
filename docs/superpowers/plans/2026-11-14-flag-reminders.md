# Flag reminders plan

Spec: docs/superpowers/specs/2026-11-14-flag-reminders-design.md. Branch
`flag-reminders` from main at the spec commit. Each task: reds first, implement,
battery, one conventional commit. One contract change: a new docs/reminders.md
with its zh mirror in the same commit, the capabilities row moves with its zh
mirror, a changelog fragment lands, and porting-backlog.md moves flag reminders
into the shipped list. No engine change. The remove-on-wake lifetime is an
escalated reference deviation (the spec states it; the PR escalates it).

### Task 1: the reminder store + the core remind/wake (D1, D2)

**Files:** a new crate crates/astra-reminders (Cargo.toml + src/lib.rs +
src/store.rs + src/migrations/0001_init.sql, copying astra-snooze's shape; the
reminder table id/created_at/account_id/provider_key/wake_at/subject/sender, an
index on wake_at; record/due/list/remove, the migration claim + malformed-skip) +
store tests; crates/astra-app/src/reminders.rs (install_reminder_store,
remind_message: refuse conversation + read the engine copy + record, NO move/flag;
wake_due_reminders(now) -> Vec<NewMailNotice>: due -> notices -> remove) +
integration tests; the App slot (lib.rs at the cap, reclaim ~4 doc lines).

- [ ] Confirm at the hook site: the astra-snooze store shape (store.rs), the
      install_snooze_store slot (snooze.rs:69, lib.rs:359), the engine-copy read
      (snooze.rs:100-126), NewMailNotice (notifications.rs:43-65). Reds first
      (store: record/due/remove round-trip, the due boundary, migration claim,
      malformed-skip, concurrent opens; core: remind records from the engine copy,
      a REGRESSION that no move/flag edit is emitted, wake answers notices +
      removes rows, no store -> no-op, conversation refused); then the store +
      core. Commit `feat: the reminder store and wake core`.
- [ ] astra-reminders is NOT cap-exempt (only the four crypto crates are); keep
      files under 500. astra-app not exempt (new child module; lib.rs at the cap,
      reclaim doc lines, no growth beyond 500). Newtypes across the boundary.
      Existing tests stay green.

### Task 2: the timer + notify + surface + contract + live proof (D3, D4, D5)

**Files:** a new client/desktop/src/reminder_timer.rs (copy snooze_timer.rs:
ReminderHost seams, 60s tick, ASTRA_REMINDER_* overrides, spawn holds the Arcs) +
fake-clock tests; notify.rs deliver_reminders + REMINDER_EVENT (no focus
suppression); the boot wiring (pass Arc<NotifyBridge> into assemble_state, open
reminders.sqlite, install_reminder_store, spawn beside snooze; headless logged());
the AppPort two methods (app_state.rs near cap reclaim doc lines, port.rs, fakes,
showcase); a new commands/reminders.rs (commands.rs at cap reclaim a doc line)
registered in lib.rs; a new client/web/src/features/reminders/ (reminderStore.ts +
ReminderDialog.tsx, the SnoozeDialog pattern, REUSE snoozeChoices) + tests; a new
services/reminders.ts (remindMessage, never ipc.ts); the row-menu remindItem
(rowMenu.ts); events/reminderFlow.ts + reminderEvents.ts + App.tsx mount feeding
notificationHistoryStore; docs/reminders.md + zh; docs/capabilities.md row + zh;
docs/changelog/unreleased/flag-reminders.md (Platforms: desktop, Bump: minor);
docs/porting-backlog.md + zh; crates/astra-app/src/tests_live_harness/
reminder_proof.rs (submit a note, remind, force wake with future now, assert the
notice subject/sender + entry gone + note STILL in INBOX unmoved, withdraw) +
tests_live_harness.rs registration.

- [ ] Reds first (the fake-clock timer tests; the notify deliver_reminders tests;
      the picker/store/event tests; the offline-skipping gated live proof); then
      the timer + notify + surface + docs. Commit
      `feat: the flag reminder surface and contract`.
- [ ] client/desktop/web not exempt: commands.rs at cap (commands/reminders.rs
      submodule, reclaim a doc line); app_state.rs 499 (reclaim ~7 doc lines, no
      AppState field, core-install); commands_fakes/port.rs 491 (keep fake docs to
      one line); ipc.ts at cap (services/reminders.ts). The reminder event does NOT
      inherit focus suppression. vitest traps: act() mid-test, mock outer functions
      whole. Comments present tense; no em/en dashes; British English; source files
      end with a newline (CI rustfmt is stricter than local). docs/zh mirrors same
      commit.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix sidecar
      test; npx tsc --noEmit && npx vitest run (client/web); cargo run -p xtask
      -- gate. If the plugin tests fail with AlreadyGranted, clean the stale
      /tmp/astra-desktop-plugin-* scratch dirs (Windows pid-recycle flake) and
      re-run. WATCH DISK SPACE (clean the cargo incremental cache if it balloons).
- [ ] Controller arms the live proof on a clean harness (scripts/dev/harness.sh
      reset, then up + env; cargo test -p astra-app
      tests_live_harness::reminder_proof -- --test-threads=1), then unset.
- [ ] Push the branch; PR (squash expected) ESCALATING the remove-on-wake
      reference deviation in the body; CI watch; squash merge, delete the branch,
      sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's store + D2's core (T1); D3's timer/notify + D4's surface +
  D5's contract + live proof + battery + PR (T2).
- Risks: the escalated remove-on-wake reference deviation (kylins keeps a
      persistent row; documented + escalated, kylins-parity follow-ups named); the
      three files at 499-500 (reclaim ~12 doc lines total); the boot wiring (pass
      NotifyBridge into assemble_state); the no-focus-suppression policy; the
      no-move/no-re-resolution wake (provider key stays valid). Device-local,
      closed-app wake precision, stale open-link (recorded gaps).
- No eval/pipeline bump (model-facing unchanged). Live-proof lesson: reset the
      harness first; the wake body takes now so the proof forces a wake without
      sleeping; clean stale plugin scratch dirs if the battery flakes; ensure
      trailing newlines (CI rustfmt).