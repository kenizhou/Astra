# Hover actions plan: quick actions on message rows

Spec: docs/superpowers/specs/2026-11-12-hover-actions-design.md. Branch
`hover-actions` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit. A contract change:
list-selection.md gains a hover-actions section with its zh mirror in the
same commit, capabilities.md moves with its zh mirror, the porting-backlog
entry resolves, and a changelog fragment lands.

### Task 1: the hover actions (D1, D2)

**Files:** client/web/src/components/message-list/MessageRow.tsx (the
Archive + Trash icon buttons beside the existing flag and read/unread
buttons in the trailing slot; dispatch intents.archiveMessage(row.key) /
intents.moveToTrash(row.key) on the hovered row only with stopPropagation;
reveal on hovered || selected || focus-within; suppress the mail-action
buttons on draft rows; keep the flagged-row persistence and the pinned row
height; aria-label + focus-visible ring per button); MessageRow.test.tsx or
MessageList.test.tsx (the reds: hover reveal, focus-within, exact archive/
trash payloads on message and thread keys, click does not select/open,
draft suppression, flagged persistence unchanged).

- [ ] Tests first; then the buttons and the reveal. Commit
      `feat: quick actions on message-row hover`.
- [ ] Cap watch: MessageRow.tsx is 234 lines; the buttons plus the reveal
      stay well under 500 (extract a small sibling only if it crosses).
      ipc.ts untouched; no new intent. The existing flag/read-unread hover
      assertions stay green.

### Task 2: the contract + close-out (D3)

**Files:** docs/list-selection.md (a hover-actions section: the single-row
target, the reveal rule, the draft suppression) + docs/zh/list-selection.zh-CN.md;
docs/capabilities.md (a new Mail-area row "Quick actions on message-row
hover", ✅) + docs/zh/capabilities.zh-CN.md; docs/porting-backlog.md (the
hover-actions entry resolves) + its zh mirror if one exists;
docs/changelog/unreleased/hover-actions.md (Platforms: desktop, Bump:
minor).

- [ ] The docs + zh + row + backlog + fragment. Commit
      `docs: the hover actions contract`.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate. No live model proof (pure UI).
- [ ] Branch pushed; PR (squash expected); CI watch (ci-ok, desktop,
      sidecar); squash-merge, delete branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's set + target + D2's reveal (T1); D3's contract + row
  + backlog + fragment + battery + PR (T2).
- Risks: a button click must never select/open the row (stopPropagation);
  the target is the hovered row alone, never the multi-selection; draft
  rows suppress the mail four; the flagged-row persistence and the pinned
  row height stay; the focus-within reveal must not make rows themselves
  focusable (the ribbon/palette/menu stay the keyboard paths); the swipe
  channel must stay clear (buttons inside the translated content).
- No eval, no pipeline bump (pure UI). No undo (matches the row menu and
  ribbon; swipe owns undo).