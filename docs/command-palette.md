# Command palette: the contract

**Scope.** The keyboard-driven command palette in the desktop shell: a
global overlay that lists the app's actions, filters as you type, and runs
the chosen one. The first of the kylins additions (the founding rules give
kylins the stack/UI advantage); an addition, not mail-calendar parity debt.
Binding on the web client's shell. Pure UI: it adds no wire type and no
intent, and it never sends.

## The surface

A custom overlay, no library. A text input over a flat list of commands:

- The filter is a case-insensitive substring on the label. No fuzzy
  scoring, no recents, no grouped headers (the reference ships none).
- ArrowUp and ArrowDown wrap; the selection clamps against a shrinking
  list. Enter runs the selected command and closes. Escape and a backdrop
  click close. The query and selection reset on every open.
- A11y: `role="dialog" aria-modal`, the input `aria-controls` and
  `aria-activedescendant` wired to the active option, `ul role="listbox"`
  and `li role="option" aria-selected`, an empty state. Focus moves into
  the input on open and back on close.
- A command whose target is absent is disabled with a reason (the honest
  disable idiom): the row shows the reason, carries `aria-disabled`, and
  refuses to run. Each command re-resolves its target at call time and
  no-ops safely if the target vanished since the list rendered.
- The overlay sits at the `--z-modal` tier and mounts last in the shell's
  overlay tail, so it opens above the other modals (Settings, account
  setup). Its entrance motion uses `--ease-spring`, the token the design
  system reserves for the palette alone.

## The binding

**Ctrl+Shift+P** (Cmd+Shift+P on macOS) toggles the palette. The opener is
a document keydown that respects `event.defaultPrevented`. **Ctrl+K stays
the search island's**; the palette never steals it. The binding is listed
in the settings Shortcuts inventory, which every new shortcut must join in
the same change.

## The commands

Every command maps to an action the app already ships; the palette adds no
new intent. The set:

- **Go**: Mail, Calendar, People; Inbox, Sent, Drafts, Archive, Trash (the
  role folder of the standing account, selected and dispatched as the
  folder pane does); Unified Inbox.
- **Compose**: New Email (the ribbon's no-account gate); Reply, Reply all,
  Forward on the open message (Forward carries the original's attachments),
  disabled with a reason when no message is open.
- **Mail actions** on the selected row: Mark read / unread, Flag / unflag,
  Archive, Delete (move to Trash), disabled with a reason when no row is
  selected.
- **Search mail**: opens the search island through a small registered
  opener seam (the island registers on mount, clears on unmount; the call
  no-ops when no island is mounted).
- **App**: Settings; Add account; Cycle theme (through the settings intent,
  so it persists); Toggle agent panel; Toggle AI documents panel; Sync mail
  now.
- **View**: Flat / Threaded; Reading pane right / bottom / off.

**Send is never a command.** Send discipline is deterministic and
human-only (rulebook, Security): sending rides the composer's own submit
path, and the palette offers no send-class verb.

## Known gaps, recorded rather than left silent

- The full shortcut engine (remappable bindings, sequences, persisted
  overrides) is out: the palette ships the one fixed binding, the shape
  open for a keyMap later.
- The shortcuts-help overlay is a separate backlog item; the static
  settings Shortcuts inventory carries the list today.
- Deferred commands, named not silent: per-account switch, outbox
  navigation, list density and filter, mark as spam, delete permanently,
  undo (no Astra counterpart).

## Matrix

| Rule | Status | Where |
|---|---|---|
| The overlay: filter, wrap-around keyboard, a11y, honest disable, reset on open | ✅ | `client/web/src/features/command-palette/CommandPalette.tsx` |
| Ctrl+Shift+P opens; Ctrl+K stays the search island's | ✅ | `useCommandPaletteOpener.ts`, `AmbientIsland.tsx` |
| The command set over the existing intents and setters, no new wire type | ✅ | `commands.ts` |
| Opens above the other modals (`--z-modal`, mounts last) | ✅ | `CommandPalette.tsx`, `AppShell.tsx` |
| Send is never a command | ✅ | the send class is the composer's submit path |
| The full shortcut engine, shortcuts help, recents/fuzzy/grouping | N/A | deferred, the gaps above |