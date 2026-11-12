# Hover actions design: quick actions on message rows

**Status.** Decided 2026-11-12. Slices branch from main at this commit.

**Scope.** The next kylins addition: quick action buttons on a message
row's trailing edge, revealed on hover. Astra already ships flag and mark
read/unread there; this completes the set with archive and trash, makes
the slot keyboard-reachable, and suppresses it on drafts. It ports
kylins's hover-actions shape (the founding rules give kylins the stack/UI
advantage); an addition, not mail-calendar parity debt. Surface-side: it
dispatches the existing intents on the hovered row, adds no wire type and
no intent, and leaves ipc.ts untouched. Pure UI, so no eval and no
pipeline bump.

## D1: the action set and the target

Two new icon buttons beside the existing flag and read/unread buttons in
`MessageRow`'s trailing slot: **Archive** (`intents.archiveMessage(row.key)`)
and **Trash** (`intents.moveToTrash(row.key)`), the icons already imported.
Each dispatches on the **hovered row only**, with `stopPropagation` so a
button click never selects or opens the row, exactly the existing
`markRead`/`toggleFlag` handlers. The labels match the row menu's nouns
("Archive", "Delete" or "Move to Trash").

Single-row target is deliberate: the bulk toolbar and the row menu already
own multi-select batches (the `resolveTargets` Outlook semantics), and the
ribbon and palette act on the single selected row. A hover button visually
belongs to one row, so it acts on that row alone even when the row rides a
multi-selection. This matches kylins and Astra's own ribbon precedent.

The mail-action buttons (archive, trash, flag, read/unread) are suppressed
on `draft` rows (the `draft` prop already exists), matching kylins and the
draft menu, whose verbs are Open/Delete Draft, not the mail four. The
flagged-row persistence rule stays: a flagged row keeps its flag button
visible regardless of hover.

## D2: reveal and reachability

The slot reveals on **hover OR selection OR focus-within**. Today
`actionsVisible = hovered || selected`; the slice adds focus-within so a
keyboard user who has selected a row can tab into the buttons and they
stay revealed while focused (the row root carries `group`, the slot a
`group-focus-within` reveal, or an equivalent focus-capture state; the
implementer picks the mechanism). The buttons are real `<button>`s with an
`aria-label` each and the standing `focus-visible:ring-2 ring-ring`. The
row height stays pinned so the revealed buttons never stretch the row
(the hover jitter kylins guards).

This is not full keyboard row navigation: rows stay `role="option"`
without `tabIndex`, and the ribbon, palette, and row menu remain the
primary keyboard paths to mail actions. Making rows arrow-key focusable is
a larger list-a11y slice, deferred. Touch users (no hover) keep swipe
actions, the row menu, and the bulk toolbar; the hover slot is mouse and
keyboard chrome and cannot collide with swipe (touch produces no hover,
and the buttons live inside the translated content, clear of the swipe
reveal).

## D3: the contract

`docs/list-selection.md` binds every surface shipping a message list; the
slice adds a short hover-actions section there (the single-row target, the
reveal rule, the draft suppression) and mirrors it to zh. A new Mail-area
capabilities row lands ("Quick actions on message-row hover"), with its zh
mirror. The porting-backlog "hover actions" entry resolves with the slice.
A changelog fragment lands.

## Known gaps recorded, not hidden

- Multi-select batch hover actions are out: the bulk toolbar and row menu
  own batches.
- User customisation of the button set and a master toggle (kylins's
  settings KV config) is a separate slice.
- No undo toast for hover archive/trash: the row menu and ribbon offer no
  undo either (only swipe does); matching them keeps the surfaces
  behaviour-identical. Reusing the swipe undo for hover is a clean
  follow-up.
- Snooze and pin candidates are out (Astra has neither feature yet).
- Full keyboard row navigation (arrow-key focusable rows) is a larger
  list-a11y slice.

## Test-first

Every behaviour lands with its red, over the MessageRow/MessageList test
patterns (the `row()` fixture, the mocked ipc dispatch, `within(listbox)`
scoping). The reds: hover reveals the archive/trash buttons
(`fireEvent.mouseEnter`/`mouseLeave`); focus-within keeps them revealed;
archive and trash dispatch the exact intent payloads on a message key and
a thread key; a button click does not select or open the row (`onSelect`
not called); draft rows suppress the mail-action buttons; the flagged-row
persistence is unchanged; the existing flag and read/unread hover
assertions stay green. Any mid-test store mutation under fake timers rides
`act()`. No live model proof (pure UI); the offline battery is the proof.