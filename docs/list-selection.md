# List selection: the Astra Mail contract

**Scope.** How the message list selects rows and acts on the lot: the
selection model, the bulk toolbar, the batch loops, the move-to-folder
picker, and what Delete honestly means. Binding on every surface that ships
a message list. Ported in substance from mail-calendar's and kylins'
selection contracts, scoped to what Astra Mail ships today; the gaps are
recorded at the end, never left silent.

**Principle.** *A selection is a set of row keys the core already knows.*
The surface holds no copies of the rows, and every action names whole rows
through the same intents the single-row menu dispatches.

## The selection model

Outlook semantics over the rendered list:

- **Ctrl** toggles one row in or out; the anchor follows the row added.
- **Shift** ranges from the anchor to the clicked row over the rendered
  items, in either direction. Date headers never count: the range runs over
  rows only.
- **Ctrl+A** takes every loaded row.
- A right-click inside the selection keeps it as the action's targets; one
  outside collapses the selection to the clicked row.

The anchor is the row the last additive gesture landed on; a toggle that
removes the anchored row moves the anchor to what remains.

## The bulk toolbar

While a selection stands, the list raises its own toolbar: the count,
Select All, Clear, and the actions, so the lot never depends on the context
menu. The actions are the same ones the row menu loops: read and unread,
flag, archive, and Delete (which is move-to-trash; see below). Select All
and Ctrl+A take the loaded rows: the list is windowed, and the window grows
as the list scrolls.

## The batch loops

A batch action fires one intent per row, fire-and-forget; the snapshots
carry the outcomes back. Each edit rides the engine's outbox, so the store
shows the change at once and the server takes it when it can; a refusal
lands as the republished list's unchanged rows, never a silent success.

## Move to folder

The row menu's `Move to folder` offers the account's other folders as a
flyout; one choice moves the whole target set in one batch, one move edit
per row through the outbox. The picker honours the move's own constraints:

- **One account.** Moves stay within the row's own account; the item hides
  for a selection that spans accounts.
- **No conversations.** A conversation row disables the item with its
  reason, because a move batch can name only messages.
- **The current folder is not offered.** A move onto the folder a row
  already sits in is a no-op the app answers without queueing anything.

## The trash-versus-delete honesty

Delete in a folder is move-to-trash: the rows land in the account's Trash,
recoverable from there. Nothing in the list deletes for good. The permanent
delete is the Trash's own verb (the folder menu inside the Trash and, for
the mail itself, the Trash folder's rows), behind its own confirm. The bulk
toolbar's Delete follows the same rule as the row menu's.

## Known gaps

- **A conversation cannot be moved**; the verb disables with its reason.
- **Select All takes the loaded window**, not the folder's whole contents;
  the list is windowed and the window grows only as the list scrolls.
- **No undo**: a batch act lands through the outbox and the way back is the
  Trash, not a keystroke.
