# Shortcuts help design: the keyboard reference overlay

**Status.** Decided 2026-11-12. Slices branch from main at this commit.

**Scope.** The next kylins addition after the command palette: a read-only
keyboard-shortcuts reference overlay, opened by `?`. It lists the shortcuts
the app already wires, grouped by where they answer, and closes on Escape
or a backdrop click. It reuses the settings Shortcuts inventory as the one
source of truth and mirrors the palette's tiny-store and opener patterns.
Landing it closes the command palette's named "shortcuts-help overlay"
gap. Pure UI: no wire type, no intent, no model-facing change, so no eval
and no pipeline bump.

## D1: the overlay

A store-driven overlay under `client/web/src/features/shortcuts-help/`:

- `ShortcutsHelp.tsx`: the wrapper reads `isOpen`, returns null closed, and
  mounts a fresh surface per open. The surface is a read-only dialog
  (`role="dialog" aria-modal aria-label="Keyboard shortcuts"`,
  `tabIndex={-1}` with `useDialogFocus` so focus enters on open and
  restores on close, and Escape answers reliably). It lists the inventory
  rows grouped by their `context` in first-appearance order (Everywhere,
  Command palette, Search island, Settings, Folder drawer, Menu bar,
  Composer, Calendar event), each row the action beside its keycap. A
  header hint reads "Press ? to reopen. Esc closes." Escape on the dialog
  and a backdrop mousedown on the self target close. The overlay sits at
  `--z-modal` and mounts after the palette in the shell's overlay tail, so
  it wins the tie when both stand open. It scrolls at a max height. It
  does NOT use `--ease-spring` or `palette-pop-in` (the design system
  reserves those for the palette alone); a plain panel fade or no motion
  is the compliant choice.
- `useShortcutsHelp.ts`: a verbatim mirror of the palette store
  (`isOpen`/`open`/`close`/`toggle` plus the three module-scope verbs).

The data is the settings `ShortcutsDetail.tsx` inventory: export
`SHORTCUTS` and `ShortcutEntry` (the table stays in place as the single
source), and reuse its keycap styling. No second copy of the shortcut
list.

## D2: the `?` binding

`useShortcutsHelpOpener.ts` mirrors the palette opener with the differences
a bare character needs: it matches the logical `event.key === '?'` with no
ctrl/meta/alt, respects `event.defaultPrevented`, bails on `event.repeat`,
and rejects an editable target (the focused element is an input, textarea,
or contenteditable, the `closest('input, textarea, [contenteditable="true"]')`
idiom). It does not test `shiftKey` (`?` is Shift+/ on standard layouts, so
the physical event carries shift while the logical key is already `?`).
Toggle semantics: a second `?` closes. While the palette input is focused
the editable guard already blocks `?`. This is Astra's first bare-character
document binding, so the guard is new and pinned by tests.

The `?` row joins the `ShortcutsDetail` inventory in the same change (its
header comment binds every new shortcut to that table): `{ keys: '?',
action: 'Open the shortcuts help', context: 'Everywhere' }`. The inventory
test pin moves from 16 rows to 17 and gains a `?` assertion.

## D3: closing the palette's gap

The command palette recorded the shortcuts-help overlay as a deferred gap.
Landing it updates, in the same change: the palette's capabilities note,
`docs/command-palette.md`'s known gaps and matrix (the shortcuts-help row
moves from deferred to shipped, pointing at shortcuts-help.md), and their
zh mirrors. A new Mail-area capabilities row for the shortcuts help lands
beside the palette's, with its zh mirror.

## Known gaps recorded, not hidden

- The shortcut engine (remappable bindings, sequences, persisted
  overrides, mac/win display variants) stays out; the inventory is the
  static single-string Ctrl notation it is today.
- No in-overlay search or filter, no recents, nothing interactive beyond
  close.
- A palette command that opens help is a natural follow-up but widens this
  slice; deferred, named.
- Focus discipline is initial focus plus restore (the palette's own), not
  a full trap. The overlay is shell-only, like the palette (not the pop-out
  composer or reader windows).

## Test-first

Every behaviour lands with its red. The overlay test (the
CommandPalette.test.tsx pattern): closed renders null; the dialog is named
and modal; the rows group by context and reuse the inventory; the z-tier
pin (`--z-modal`, not the backdrop tier); focus in on open and back on
close; Escape closes; the backdrop self-target closes but an inside click
does not. The store test: open/close/toggle and the module verbs. The
opener test: `?` opens and a second `?` closes; a claimed key
(defaultPrevented) is left alone; `?` does not fire while an input,
textarea, or contenteditable is focused; Ctrl+K and Ctrl+Shift+P are
unaffected. The inventory pin moves to 17 with a `?` assertion. Any
mid-test store mutation under fake timers rides `act()`. No live model
proof (pure UI); the offline battery is the proof.