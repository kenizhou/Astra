# SP2b Message List Design

Date: 2026-10-09
Status: proposed
Parent spec: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (sections 5, 15)

## 1. Purpose

Make the message list pane real: rows for the selected folder, flat and
threaded view modes, windowed paging, unread and flag display with read and
flag toggles, and monogram avatars. Reading (bodies, the reading pane) is
the next slice; this one ends at "the list is a mail list".

## 2. Decisions

- **D1**: the MailboxList snapshot grows the message list additively:
  `rows`, `selection`, `visible_limit`, `view_mode` join `account_folders`;
  existing fixtures stay valid (new fields serde-default to empty).
- **D2**: two new Intents ported from mail-calendar's message actions in
  their single-item shapes: `MarkRead { row, read }` and `SetFlagged { row,
  flagged }`, over a `RowRef` enum (`Message(MessageRef)` | `Thread(
  ThreadRef)`) added to `astra-app::reference` with `from_parts`
  construction. Bulk selection and the remaining actions (delete, archive,
  move) arrive with their slices.
- **D3**: list reads follow mail-calendar: flat mode via engine
  `mail_window`, threaded mode via `mail_on_threads`, projected through the
  SP1 row cache with the existing generation guard; `ShowMore` grows the
  window (PAGE already 100); `SetViewMode` switches the projection and
  resets the window.
- **D4**: read and flag writes go through engine `edit_mail` with
  mail-calendar's discard-and-resync posture: the mutation result is not
  surfaced directly; the optimistic row state plus a re-sync publish the
  outcome.
- **D5**: the frontend adds `@tanstack/react-virtual` (kylins' choice) for
  the list, monogram avatars from the sender address (kylins pattern),
  unread weight, flag glyph, relative dates, a view mode toggle, and a
  ShowMore affordance; actions act on the single selected row for now.

## 3. Testing

- Unit (astra-app, in-memory engine + fakes): folder selection rebuilds
  rows; ShowMore grows; view mode switch re-projects and resets; MarkRead
  and SetFlagged write through edit_mail and the row republishes; row cache
  invalidation on sync.
- Wire: snapshot fixtures with rows pinned both sides (Rust json + TS).
- Frontend (vitest): MessageList renders rows from a fixture (virtualized
  window), toggle view mode dispatches the intent, read/flag buttons
  dispatch with the right RowRef payload.
- Live (gated, harness): the harness seeds three messages into astra1's
  INBOX (delivered via the harness SMTP submission so the path is real);
  after RefreshMail the list test asserts three rows, unread then read
  after MarkRead.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build and vitest
   green.
2. Folder selection shows that folder's rows; ShowMore pages; flat and
   threaded modes both render (threaded grouped under the thread root with
   a count).
3. MarkRead and SetFlagged visibly update the row and survive a RefreshMail
   (engine write verified by the gated live test).
4. `docs/capabilities.md` rows move (message list to partial, threading to
   partial for the grouped view) with the zh mirror and a changelog
   fragment in the same change.
5. No protocol breaks: existing fixtures and wire tests pass unchanged
   (additive only).
