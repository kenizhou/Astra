# Sweep design: clean up a sender

**Status.** Decided 2026-11-12. Slices branch from main at this commit.

**Scope.** The next kylins addition: sweep, a one-gesture clean-up of one
sender's mail. Kylins's sweep is "purely an orchestration shell over
existing seams"; Astra's already has every ingredient (the list rows with
sender_address, the per-row batch intents the bulk toolbar loops, the row
menu, the dialog idiom). This slice ships the bounded current-folder
version: a "Clean up sender" dialog that previews how many of the loaded
rows are from the sender, then archives or trashes them in one confirmed
batch. Surface-side: no new wire type, no new intent, no engine change, so
no eval and no pipeline bump. (The attachment browser was assessed and
skipped: a true cross-mail grid needs a heavy engine/host seam, and a
filtered view duplicates the shipped Attachments tab.)

## D1: the entry and the dialog

A row-menu entry, "Clean up sender…", opens a `SweepDialog` prefilled with
the row's `sender_address`. The dialog reads the current folder's loaded
rows (`listStore.rows`) and filters them by `sender_address` (the exact
`listFilter.ts` predicate pattern), so the preview is the count of matching
rows in view, honest about the loaded window (not the folder's whole
contents). It shows the sender, the count, a bounded preview of the matching
subjects, and an action choice: **Archive** (the default) or **Move to
Trash**. A confirm button runs the batch; Cancel closes without dispatching.

On confirm, the dialog loops the chosen intent over the matching rows
(`intents.archiveMessage(row.key)` or `intents.moveToTrash(row.key)`), the
bulk toolbar's own `forEach` batch pattern, then closes. The act verbs stop
at the row's trash (no permanent delete; the rulebook's trash ceiling). The
dialog is keyboard-reachable (a real dialog with the shared focus idiom,
Escape closes, the action choice and confirm are buttons) and uses the
standing focus-visible ring.

## D2: the scope boundary

v1 sweeps the **current folder's loaded rows** only. It does not run a
cross-folder search (that would temporarily hijack the list mode and read a
search snapshot); the cross-folder sweep, orchestrating the search intent
over `from:<addr> mailbox:<id>`, is a named follow-up. The preview therefore
says "in this folder" and is honest that it covers the loaded window, the
list's own windowing discipline.

No undo in v1: the bulk toolbar and the row menu offer no undo either (only
swipe does), so sweep matches them; reusing the swipe undo for sweep is a
clean follow-up. The confirm dialog is the guard against a mistaken sweep.

## D3: the contract

docs/list-selection.md binds the list surfaces; the slice adds a short sweep
section there (the current-folder scope, the preview honesty, the confirmed
batch, the trash ceiling, the no-undo stance) and mirrors it to zh. A new
Mail-area capabilities row lands, with its zh mirror. The porting-backlog
"sweep" entry resolves with the slice. A changelog fragment lands.

## Known gaps recorded, not hidden

- Current folder, loaded window only; a cross-folder sweep over the search
  intent is a named follow-up.
- No undo (matches the bulk toolbar and row menu; swipe owns undo); the
  confirm is the guard.
- Archive and Move to Trash only; mark-read sweep and a custom action set
  are out.
- The preview covers the loaded window, not the folder's whole contents
  (the list is windowed); the count says so.

## Test-first

Every behaviour lands with its red, over the list/dialog test patterns (the
mocked ipc dispatch, the row fixtures, within-scoped queries). The reds: the
row menu carries the "Clean up sender" entry and opens the dialog prefilled
with the sender; the dialog counts the matching rows in the loaded folder
(and excludes other senders); the action choice defaults to Archive; confirm
dispatches the EXACT archiveMessage / moveToTrash payload for each matching
row (and only those); Cancel dispatches nothing; Escape closes; an empty
match disables confirm with its reason. Any mid-test store mutation under
fake timers rides act(). No live model proof (pure UI); the offline battery
is the proof.