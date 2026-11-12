# Quick steps design: one-click mail macros

**Status.** Decided 2026-11-12. Slices branch from main at this commit.

**Scope.** The next kylins addition: quick steps, user-defined macros that
run a mail action over the selected rows in one click. Kylins's quick step
is "one filing verb plus an optional mark-read-first", not a multi-action
sequence engine; Astra ports that shape over the verbs it already ships.
Surface-side: no new wire type, no new intent, no engine, host, or Rust
work, so no eval and no pipeline bump. Every ingredient exists (the per-row
batch intents, the bulk loop, the move flyout's eligibility rules, the
device-local store idiom, the ribbon primitives, the dialog idiom).

## D1: the model and the store

A `quickStepsStore` (zustand + localStorage, the notificationHistoryStore
idiom: defensive parse, persist logged never thrown, hydrate at module
load). A quick step is `{ id, name, action, target?, markRead? }`:

- `action` is one of `archive`, `trash`, `mark-read`, `mark-unread`, `flag`,
  `unflag`, `move`.
- `target` (move only) is a concrete folder `{ account, folderKey }` from
  the folder store; role targets are a named follow-up.
- `markRead` is the optional mark-read-first modifier (kylins's shape).
- A fixed cap of five steps (kylins's MAX_QUICK_STEPS); the store offers
  upsert (add/edit), delete, and the ordered list. Malformed entries drop
  on parse.

## D2: the runner

`runQuickStep(step, rows)` maps the action to the existing intent (the
rowSwipe action-to-intent table idiom) and loops it over the rows (the bulk
toolbar's forEach):

- archive → `intents.archiveMessage`, trash → `intents.moveToTrash`,
  mark-read/unread → `intents.markRead`, flag/unflag → `intents.setFlagged`,
  move → `intents.moveMessages` (the folders.ts feature service).
- `markRead` runs FIRST, over the eligible rows, because filed rows leave
  the snapshot (kylins's rationale).
- move skips conversation (thread) rows and foreign-account rows with a
  counted toast (the swipeToastStore idiom): `moveMessages` cannot name a
  conversation (the row menu's own rule), and a concrete folder belongs to
  one account.
- The runner acts on `listStore.selectedKeys` (the bulk selection) with the
  single-anchor fallback the ribbon uses; nothing selected runs nothing.

## D3: the manager and the ribbon

A `QuickStepsManager` dialog (the SweepDialog chrome: store-gated,
useDialogFocus, Escape/backdrop close, --z-modal) lists the saved steps and
an inline form to create/edit one: a name, the action select, a folder
select for move (a plain select over the folder store, no tree picker), the
mark-read checkbox, and delete. The five-cap disables add at the limit with
its reason.

A `QuickStepsRibbonGroup` (the RibbonPrimitives idiom) mounts in the
ReadRibbon: one button per saved step labelled by name, disabled without a
selection, running the step over the selection; a Manage button opens the
dialog. The group hides in compact mode like the other ribbon groups.

## D4: the contract

docs/list-selection.md binds the list actions; the slice adds a quick-steps
section there (the model, the runner's mark-read-first and move-skip rules,
the cap, the device-local persistence) and mirrors it to zh. A new Mail-area
capabilities row lands, with its zh mirror. The porting-backlog "quick
steps" entry resolves with the slice. A changelog fragment lands.

## Known gaps recorded, not hidden

- The snooze action is out: it depends on unshipped snooze infrastructure
  (hide/resurface scheduling, host timers). The only kylins action with an
  unshipped dependency.
- Keyboard slots (quickstep:1..5) are out: Astra has no shortcut engine yet
  (the command palette's named gap).
- Role targets and multi-leg sequences are out (kylins does not do sequences
  either); concrete-folder move only.
- Palette commands for the steps are a cheap optional follow-up, not v1.
- Steps are device-local (localStorage), not synced across devices.
- No undo (matches the bulk toolbar; the filing verbs stop at trash).

## Test-first

Every behaviour lands with its red. The store test: upsert/delete, the
five-cap, the defensive parse (malformed drops), the localStorage
persistence. The runner test: each action dispatches the EXACT intent
payload over the rows; mark-read runs before the filing verb; move skips
thread rows and foreign-account rows with the counted toast; nothing
selected runs nothing. The manager test: create/edit/delete, the cap
disables add, the folder select for move. The ribbon test: a button per
step, disabled without a selection, runs over the selection, Manage opens
the dialog. Any mid-test store mutation under fake timers rides act(). No
live model proof (pure UI); the offline battery is the proof.