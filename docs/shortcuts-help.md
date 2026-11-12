# Shortcuts help: the contract

**Scope.** The read-only keyboard-shortcuts reference overlay in the
desktop shell, opened by `?`. The kylins addition after the command
palette; it lists the shortcuts the app already wires, grouped by where
they answer. Binding on the web client's shell. Pure UI: it adds no wire
type and no intent, and it reuses the settings Shortcuts inventory as the
one source of truth.

## The surface

A store-driven overlay, no library, mirroring the palette's shape:

- The wrapper reads `isOpen`, returns null closed, and mounts a fresh
  surface per open. The surface is a read-only dialog (`role="dialog"
  aria-modal aria-label="Keyboard shortcuts"`, `tabIndex={-1}` with the
  shared focus hook so focus enters on open and restores on close).
- The rows are the settings `ShortcutsDetail` inventory, grouped by their
  `context` in first-appearance order (Everywhere, Command palette, Search
  island, Settings, Folder drawer, Menu bar, Composer, Calendar event),
  each row the action beside its keycap. The inventory is exported and
  reused, never copied.
- Escape on the dialog and a backdrop mousedown on the self target close.
  A header hint reads "Press ? to reopen. Esc closes." The overlay scrolls
  at a max height.
- The overlay sits at the `--z-modal` tier and mounts after the palette in
  the shell's overlay tail, so it wins the tie when both stand open. It
  does not use the palette's reserved spring easing; a plain panel is the
  compliant choice.

## The binding

**`?`** toggles the overlay. The opener is a document keydown that matches
the logical `event.key === '?'` with no ctrl/meta/alt, respects
`event.defaultPrevented`, bails on `event.repeat`, and rejects an editable
target (the focused element is an input, textarea, or contenteditable). It
does not test `shiftKey`: `?` is Shift+/ on standard layouts, so the
physical event carries shift while the logical key is already `?`. While
the palette input is focused the editable guard already blocks `?`. This
is the shell's first bare-character document binding, so the guard is
pinned by tests. The binding is listed in the settings Shortcuts inventory
(the `?` row, context Everywhere), which every new shortcut must join in
the same change.

## Known gaps, recorded rather than left silent

- The shortcut engine (remappable bindings, sequences, persisted
  overrides, mac/win display variants) stays out; the inventory is the
  static single-string Ctrl notation it is today.
- No in-overlay search or filter, no recents, nothing interactive beyond
  close.
- A palette command that opens help is a natural follow-up, deferred to
  keep this slice bounded.
- Focus discipline is initial focus plus restore (the palette's own), not
  a full trap. The overlay is shell-only, like the palette.
- Escape answers on the topmost overlay's own dialog; the pre-existing
  document-level Escape listeners (Settings, folder drawer) do not check
  `defaultPrevented`, so Escape can close a stacked layer beneath. The
  palette behaves identically; a topmost-only Escape is a shared overlay
  convention for a later slice, not this one.

## Matrix

| Rule | Status | Where |
|---|---|---|
| The read-only overlay grouped by context, reusing the inventory | ✅ | `client/web/src/features/shortcuts-help/ShortcutsHelp.tsx` |
| `?` opens and toggles; the editable-target guard; Ctrl+K and Ctrl+Shift+P unaffected | ✅ | `useShortcutsHelpOpener.ts` |
| Focus in on open and back on close; Escape and backdrop close | ✅ | `ShortcutsHelp.tsx`, `useDialogFocus.ts` |
| Opens above the other modals (`--z-modal`, mounts after the palette) | ✅ | `ShortcutsHelp.tsx`, `AppShell.tsx` |
| The `?` row in the settings Shortcuts inventory | ✅ | `ShortcutsDetail.tsx` |
| The shortcut engine, in-overlay search, a palette command opening help | N/A | deferred, the gaps above |