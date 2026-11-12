# Command palette plan: the first kylins addition

Spec: docs/superpowers/specs/2026-11-12-command-palette-design.md. Branch
`command-palette` from main at the docs commit. Each task: reds first, the
implementation, the battery, a conventional commit. A contract change:
capabilities.md moves with its zh mirror in the same commit, a new
docs/command-palette.md (+ zh) is the Source, and a changelog fragment lands.

### Task 1: the palette feature (D1, D2)

**Files:** client/web/src/features/command-palette/CommandPalette.tsx (the
overlay: substring filter, wrap-around arrows, Enter runs + closes,
Escape/backdrop close, listbox/option a11y, empty state, reset-on-open,
jsdom-safe scrollIntoView, honest-disable rows, --ease-spring entrance);
useCommandPalette.ts (the tiny store + module-scope open/close/toggle);
commands.ts (the registry of { id, label, category, icon, run, disabled? }
from existing store getters and intents; no new wire type, no new intent);
components/layout/AppShell.tsx (mount in the overlay tail); the document
keydown opener (Ctrl+Shift+P, respects defaultPrevented; Ctrl+K stays the
island's); the small search-island opener seam; features/settings/
ShortcutsDetail.tsx (the binding row in the SHORTCUTS table);
CommandPalette.test.tsx + useCommandPalette.test.ts (the reds).

- [ ] Tests first (filter, keyboard nav + execute dispatches the right
      intent via mocked ipc, escape/backdrop close, disabled gates,
      reset-on-open, empty state, the Ctrl+Shift+P opener); then the
      component, store, registry, mount, and seam. Commit
      `feat: the command palette`.
- [ ] Cap watch: keep each file under 500; if commands.ts grows past it,
      split the registry by category into a child module. ipc.ts stays
      untouched. No send command anywhere. Folder names never collide up
      to case (CommandPalette.tsx beside useCommandPalette.ts is safe).

### Task 2: the contract + close-out

**Files:** docs/command-palette.md (the contract: the surface, the binding,
the command set, the send refusal, the deferred commands) + docs/zh/
command-palette.zh-CN.md; docs/capabilities.md (a new Mail-area row in the
main table, ⬜ to ✅ as it ships complete) + docs/zh/capabilities.zh-CN.md;
docs/changelog/unreleased/command-palette.md (Platforms: desktop, Bump:
minor).

- [ ] The docs + zh + row + fragment. Commit `docs: the command palette contract`.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate. No live model proof (pure UI).
- [ ] Branch pushed; PR (squash expected); CI watch (ci-ok, desktop,
      sidecar); squash-merge, delete branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's surface + D2's binding and command set (T1); the
  contract + row + fragment + battery + PR (T2).
- Risks: the palette must not steal Ctrl+K from the search island (the
  opener respects defaultPrevented and binds Ctrl+Shift+P only); the
  mail-action and compose commands must disable-with-reason when their
  target is absent, never dispatch against nothing; the theme command must
  persist through the settings intent, not the local setter; send must
  never appear; the search seam must not break the island's own Ctrl+K.
- No eval, no pipeline bump (pure UI, no model-facing change). The send
  refusal is recorded in the contract.