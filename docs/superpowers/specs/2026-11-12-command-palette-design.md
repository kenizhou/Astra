# Command palette design: the first kylins addition

**Status.** Decided 2026-11-12. Slices branch from main at this commit.

**Scope.** The first of the kylins additions the backlog names
(docs/porting-backlog.md:70-75), now picked under the owner's standing
delegation to run the queue. A keyboard-driven command palette: a global
overlay that lists the app's actions, filters as you type, and runs the
chosen one. It ports kylins's UI shape (the founding rules give kylins the
stack/UI advantage); it is an addition, not mail-calendar parity debt, and
it adds a new capabilities row rather than closing a partial one. Pure UI:
no model-facing change, so no eval run and no pipeline bump.

## D1: the palette surface

A custom overlay, no new dependency (kylins ships none; Astra has none).
Three pieces under `client/web/src/features/command-palette/`:

- `CommandPalette.tsx`: the overlay. A text input over a flat list of
  commands. Case-insensitive substring filter on the label (kylins's own
  "no fuzzy scoring, YAGNI" discipline); ArrowUp/Down wrap; Enter runs the
  selected command and closes; Escape and backdrop click close; the
  selection index clamps against a shrinking list; query and selection
  reset on every open; `scrollIntoView({block:'nearest'})` guarded for
  jsdom. A11y mirrors kylins: `role="dialog" aria-modal`, the input
  `aria-controls`/`aria-activedescendant`, `ul role="listbox"` /
  `li role="option" aria-selected`, an empty state. Each row carries its
  icon and label; a disabled command shows its reason (the ContextMenuItem
  honest-disable idiom), never a silent no-op.
- `useCommandPalette.ts`: a tiny zustand store (`isOpen`, `open`, `close`,
  `toggle`) plus module-scope `openCommandPalette`/`closeCommandPalette`/
  `toggleCommandPalette` so non-React code can toggle it.
- `commands.ts`: the registry, a list of `{ id, label, category, icon, run,
  disabled? }` built from the existing store getters and intents. Every
  command maps to an action Astra already ships; the palette adds no new
  wire type and no new intent (so ipc.ts stays untouched, per the rulebook
  trap).

The overlay mounts in `AppShell`'s overlay tail (beside SettingsScreen).
Its entrance motion uses `--ease-spring`, the token the design system
reserves for the palette alone (docs/design-system.md:39-41).

## D2: opening and the command set

**Ctrl+Shift+P** opens the palette (kylins parity). Ctrl+K stays the search
island's (AmbientIsland.tsx:113-126); the palette's document keydown
respects `event.defaultPrevented`. The binding lands in the static
`ShortcutsDetail.tsx` SHORTCUTS table in the same change (its comment binds
new shortcuts to that inventory).

The command set, each mapped to an existing action:

- **Go**: Mail / Calendar / People (`setShellView`); Inbox / Sent / Drafts /
  Archive / Trash (find the folder by role in the standing account, then
  the `select` + `dispatch(intents.selectFolder)` pair FolderTree uses);
  Unified Inbox (`selectUnified` + `selectAccount(null)`).
- **Compose**: New Email (`openNewMessageComposer`, the same no-account gate
  the ribbon uses); Reply / Reply all / Forward on the open message
  (`intents.replyTo`/`replyAllTo`/`forward` with the carried attachments),
  disabled with a reason when no message is open.
- **Mail actions** on the selected row: Mark read / Mark unread, Flag /
  Unflag, Archive, Delete (move to Trash), each the intent the ribbon and
  row menu already dispatch, disabled with a reason when no row is
  selected.
- **Search mail**: opens the search island through a small new seam (the
  island's expanded state is local today; a module-scope opener or a
  uiStore flag, the implementer's choice, kept minimal).
- **App**: Settings (`setSettingsOpen`); Add account (`setAccountSetupOpen`);
  Cycle theme (through `intents.setTheme(nextThemeMode(...))`, never the
  local setter alone, so it persists); Toggle agent panel; Toggle AI
  documents panel; Sync mail now (`intents.refreshMail`).
- **View**: Flat / Threaded (`intents.setViewMode`); Reading pane right /
  bottom / off (`appearance.setReadingPosition`, device-local).

**Send is never a palette command.** Send discipline is deterministic and
human-only (rulebook, Security); sending rides the composer's own submit
path. Delete-permanently and spam are deferred (they carry their own
confirm/gate); undo has no Astra counterpart; per-account switch, outbox
navigation, and density/filter commands are deferred to keep the first
slice bounded.

## Known gaps recorded, not hidden

- The full shortcut engine (remappable bindings, sequences, persisted
  overrides, kylins's services/shortcuts stack) is out: the palette ships
  the one fixed binding, and the shape stays open for a keyMap later.
- The shortcuts-help overlay is a separate backlog item; Astra's static
  ShortcutsDetail already carries the inventory.
- No recents, fuzzy scoring, or grouped headers (kylins itself ships none).
- The deferred commands above (per-account switch, outbox, density/filter,
  spam, delete-permanently) are named, not silent.

## Test-first

Every behaviour lands with its red. The component test (vitest +
@testing-library/react, the SettingsScreen.test.tsx overlay pattern): the
filter narrows the list; ArrowUp/Down wrap and Enter runs the selected
command (asserting the right intent through a mocked ipc); Escape and
backdrop click close; a disabled command shows its reason and does not
dispatch; query and selection reset on reopen; the empty state renders. The
store test: open/close/toggle and the module-scope helpers. Any mid-test
zustand mutation under fake timers rides `act()` (the rulebook trap). The
keyboard opener test: Ctrl+Shift+P opens, Ctrl+K still opens the island. No
live model proof (pure UI); the offline battery is the proof.