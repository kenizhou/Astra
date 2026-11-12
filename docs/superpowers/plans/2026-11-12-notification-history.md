# Notification history plan: the bell and its record

Spec: docs/superpowers/specs/2026-11-12-notification-history-design.md.
Branch `notification-history` from main at the docs commit. Each task: reds
first, the implementation, the battery, a conventional commit. A contract
change: background-sync.md gains a history clause with its zh mirror in the
same commit, capabilities.md moves with its zh mirror, the porting-backlog
entry resolves, and a changelog fragment lands.

### Task 1: the feature (D1, D2)

**Files:** crates/astra-app/src/notifications.rs (derive Serialize on
NewMailNotice and NewMailBatch, additive); client/desktop/src/notify.rs
(NotifyBridge::deliver emits a `new-mail` Tauri event with the batch after
the enabled/focus gate, the share.rs/mailto.rs app.emit pattern; the
headless path emits nothing; a history failure never suppresses the OS
notification); notify_tests.rs (the emit pins); client/web/src/events/
newMailEvents.ts (the StrictMode-safe mount, mailtoEvents.ts pattern) +
newMailEvents.test.ts; client/web/src/stores/notificationHistoryStore.ts
(zustand + localStorage, cap 100 newest first, dedupe by account+key,
record/dismiss/clear) + notificationHistoryStore.test.ts; client/web/src/
components/icons.tsx (BellIcon); client/web/src/components/layout/TitleBar.tsx
(the bell button); client/web/src/features/notification-history/
NotificationHistoryPanel.tsx (the popover: list newest first, click opens
via intents.openMessage, dismiss, clear all, empty state, Escape close) +
NotificationHistoryPanel.test.tsx; App.tsx (mount newMailEvents).

- [ ] Tests first (Rust emit pins; web event-mount, store, panel reds); then
      the host emit, the event mount, the store, the bell, and the panel.
      Commit `feat: a history of new-mail notifications behind a bell`.
- [ ] Cap watch: each file under 500 (the panel ~150-200, the store ~100).
      ipc.ts untouched (no new intent; openMessage already exists). No engine
      change. The serde derives are additive. The existing notify/delivery
      tests stay green.

### Task 2: the contract + close-out (D3)

**Files:** docs/background-sync.md (a history clause: the host emits the
notified batch, the web records it device-local capped at 100, the bell
reads it) + docs/zh/background-sync.zh-CN.md; docs/capabilities.md (a new
Sync-area row beside the notification row, ✅) + docs/zh/capabilities.zh-CN.md;
docs/porting-backlog.md (the notification-history entry resolves) + its zh
mirror; docs/changelog/unreleased/notification-history.md (Platforms:
desktop, Bump: minor).

- [ ] The docs + zh + row + backlog + fragment. Commit
      `docs: the notification history contract`.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate. No live model proof.
- [ ] Branch pushed; PR (squash expected); CI watch (ci-ok, desktop,
      sidecar); squash-merge, delete branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's recording path + D2's bell/panel (T1); D3's contract +
  row + backlog + fragment + battery + PR (T2).
- Risks: the emit fires only after the enabled/focus gate (history of
  notified mail, not a new-mail log) and never on the headless path; a
  history failure must not suppress the OS notification; the serde derives
  are additive (no field rename); the store dedupes by account+key and caps
  at 100 newest first; click opens through the EXISTING openMessage intent
  (no new intent, ipc.ts untouched); the panel is keyboard-reachable; the
  existing notify/delivery tests stay green.
- No eval, no pipeline bump (no model-facing change). No engine change.