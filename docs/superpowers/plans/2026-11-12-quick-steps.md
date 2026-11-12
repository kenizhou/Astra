# Quick steps plan: one-click mail macros

Spec: docs/superpowers/specs/2026-11-12-quick-steps-design.md. Branch
`quick-steps` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit. A contract change:
list-selection.md gains a quick-steps section with its zh mirror in the same
commit, capabilities.md moves with its zh mirror, the porting-backlog entry
resolves, and a changelog fragment lands.

### Task 1: the feature (D1, D2, D3)

**Files:** client/web/src/features/quick-steps/quickStepsStore.ts (the
model { id, name, action, target?, markRead? }, action set archive/trash/
mark-read/mark-unread/flag/unflag/move, concrete-folder move target, the
five-cap, upsert/delete, localStorage persistence + defensive parse, the
notificationHistoryStore idiom) + quickStepsStore.test.ts; runQuickStep.ts
(the action-to-intent table the rowSwipe idiom, the forEach bulk loop,
mark-read-first, move skips thread + foreign-account rows with a counted
swipeToastStore toast, acts on listStore.selectedKeys with anchor fallback)
+ runQuickStep.test.ts; QuickStepsManager.tsx (the dialog: list + inline
name/action/folder-select/mark-read form, edit/delete, the cap disables add,
SweepDialog chrome) + QuickStepsManager.test.tsx; QuickStepsRibbonGroup.tsx
(a button per step, disabled without selection, runs over the selection, a
Manage button; RibbonPrimitives idiom) + QuickStepsRibbonGroup.test.tsx;
ReadRibbon.tsx (mount the group, hide in compact); a manager store flag or
self-mount (the kylins pattern).

- [ ] Tests first (store CRUD/cap/parse/persist; runner exact payloads +
      mark-read-first + move skips + nothing-selected; manager create/edit/
      delete/cap; ribbon buttons/disabled/run/Manage); then the store,
      runner, manager, and ribbon. Commit
      `feat: quick steps, one-click mail macros`.
- [ ] Cap watch: each file under 500 (store ~150, runner ~120, manager
      ~250, ribbon ~80). ipc.ts untouched (moveMessages already lives in
      folders.ts). No engine/host/Rust work. No snooze action, no keyboard
      slots, no role targets, no sequences. The existing ribbon tests stay
      green.

### Task 2: the contract + close-out (D4)

**Files:** docs/list-selection.md (a quick-steps section: the model, the
runner's mark-read-first and move-skip rules, the cap, the device-local
persistence) + docs/zh/list-selection.zh-CN.md; docs/capabilities.md (a new
Mail-area row, ✅) + docs/zh/capabilities.zh-CN.md; docs/porting-backlog.md
(the quick-steps entry resolves) + its zh mirror;
docs/changelog/unreleased/quick-steps.md (Platforms: desktop, Bump: minor).

- [ ] The docs + zh + row + backlog + fragment. Commit
      `docs: the quick steps contract`.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate. No live model proof (pure UI).
- [ ] Branch pushed; PR (squash expected); CI watch (ci-ok, desktop,
      sidecar); squash-merge, delete branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's store + D2's runner + D3's manager/ribbon (T1); D4's
  contract + row + backlog + fragment + battery + PR (T2).
- Risks: mark-read runs BEFORE the filing verb (filed rows leave the
  snapshot); move skips thread rows (moveMessages cannot name a
  conversation) and foreign-account rows with a counted toast; the runner
  acts on the selection with anchor fallback, nothing selected runs nothing;
  the five-cap disables add; the store parse drops malformed entries; the
  ribbon group hides in compact; ipc.ts untouched.
- No eval, no pipeline bump (pure UI). No snooze (unshipped dependency), no
  keyboard slots (no shortcut engine), no role targets, no sequences, no
  undo (matches the bulk toolbar).