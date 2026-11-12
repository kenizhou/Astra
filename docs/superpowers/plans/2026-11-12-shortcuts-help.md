# Shortcuts help plan: the keyboard reference overlay

Spec: docs/superpowers/specs/2026-11-12-shortcuts-help-design.md. Branch
`shortcuts-help` from main at the docs commit (main carries the command
palette, PR #71). Each task: reds first, the implementation, the battery, a
conventional commit. A contract change: capabilities.md moves with its zh
mirror in the same commit, a new docs/shortcuts-help.md (+ zh) is the
Source, the palette's deferred gap is closed in the same change, and a
changelog fragment lands.

### Task 1: the overlay feature (D1, D2)

**Files:** client/web/src/features/settings/ShortcutsDetail.tsx (export
SHORTCUTS + ShortcutEntry, the table stays the single source; add the `?`
row { keys: '?', action: 'Open the shortcuts help', context: 'Everywhere' };
optionally export Kbd); ShortcutsDetail.test.tsx (the pin 16 to 17 rows + a
`?` assertion); client/web/src/features/shortcuts-help/useShortcutsHelp.ts
(the palette store mirror) + useShortcutsHelp.test.ts;
useShortcutsHelpOpener.ts (logical `?`, no ctrl/meta/alt, defaultPrevented
respect, event.repeat bail, the editable-target guard, toggle) +
useShortcutsHelpOpener.test.tsx; ShortcutsHelp.tsx (the read-only dialog
grouped by context, useDialogFocus on tabIndex=-1, Escape on the dialog,
backdrop self-target close, --z-modal, max-height scroll, no spring) +
ShortcutsHelp.test.tsx; components/layout/AppShell.tsx (the opener beside
useCommandPaletteOpener; mount the overlay after CommandPalette).

- [ ] Tests first (overlay closed/null, named modal, grouped rows, z-tier,
      focus in/out, Escape, backdrop; store; opener `?` opens/toggles,
      claimed key, editable-target guard, Ctrl+K and Ctrl+Shift+P
      unaffected; inventory pin 17); then the implementation. Commit
      `feat: the shortcuts help overlay`.
- [ ] Cap watch: each file under 500 (the overlay ~120, the opener ~40).
      No --ease-spring or palette-pop-in (palette-only). No second copy of
      the shortcut list. ipc.ts untouched.

### Task 2: the contract + close-out (D3)

**Files:** docs/shortcuts-help.md (the contract: the surface, the `?`
binding and its guard, the inventory reuse, the deferrals) + docs/zh/
shortcuts-help.zh-CN.md; docs/capabilities.md (a new Mail-area row beside
the palette's, ⬜ to ✅; the palette row's note drops shortcuts-help from
its deferred list) + docs/zh/capabilities.zh-CN.md; docs/command-palette.md
(the known gaps and matrix: shortcuts-help moves from deferred to shipped,
pointing at shortcuts-help.md) + docs/zh/command-palette.zh-CN.md;
docs/changelog/unreleased/shortcuts-help.md (Platforms: desktop, Bump:
minor).

- [ ] The docs + zh + rows + the palette gap closure + fragment. Commit
      `docs: the shortcuts help contract`.
- [ ] Full battery (direct exit codes): cargo fmt --all; cargo fmt --all --
      --check; cargo clippy --workspace --all-targets -- -D warnings; cargo
      test --workspace; npm --prefix sidecar run build && npm --prefix
      sidecar test; npx tsc --noEmit && npx vitest run (client/web); cargo
      run -p xtask -- gate. No live model proof (pure UI).
- [ ] Branch pushed; PR (squash expected); CI watch (ci-ok, desktop,
      sidecar); squash-merge, delete branch, sync main; ledger cleanup.

## Self-review notes

- Spec coverage: D1's overlay + D2's `?` binding (T1); D3's gap closure +
  the contract + row + fragment + battery + PR (T2).
- Risks: the `?` opener is Astra's first bare-character document binding,
  so the editable-target guard must block typing in inputs/textareas/
  contenteditable (and the palette input); it must not test shiftKey; it
  must respect defaultPrevented and leave Ctrl+K / Ctrl+Shift+P alone. The
  overlay must use --z-modal and mount after the palette (the z-tie). The
  inventory stays the single source (export, not copy). The palette's
  deferred mentions must all move (capabilities note, command-palette.md
  gaps + matrix, zh).
- No eval, no pipeline bump (pure UI). No spring easing (palette-only).