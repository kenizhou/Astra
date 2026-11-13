# Attachment browser: the Astra Mail contract

**Scope.** A cross-message attachment browse: a transient modal listing
attachments across mail, grouped by sender, filtered by mime, with Save As and
open-the-source-message. Binding on every surface that ships the browse. The
engine has no cross-message attachment index, so this is a bounded fan-out over
the engine's existing seams (search `has_attachment`, resolve,
`message_attachments`), a command returning a list beside a coverage report
(never a new Surface), and a modal overlay (never a fourth view). No engine
change. The gaps are recorded at the end, never left silent.

**Principle.** *The browse is a command, not a Surface, and never a folder.* It
recomputes per open and publishes nothing: there is no snapshot to signal and no
persistent index to keep fresh. The shell's rail and `ShellView`
(mail/calendar/people) are untouched; the modal rides the shell's tail beside the
command palette, the shortcuts help's own overlay pattern.

## Reference deviation, escalated

The locked kylins reference (the attachment index, the attachment-browser
feature) builds a rebuildable SQLite index with a cache-only sweep: it reads
cached raw sources and never dials the server (a miss is counted skipped).
Astra's only engine seam (`message_attachments`) is cache-first but fetches on a
miss, and engine-api exposes no cache-only attachment read. This slice ships
fetch-on-miss with a bound (no engine change): the first open warms the scanned
messages' sources over the network, bounded by the scan cap, and the coverage
footer states the window honestly. This is a deliberate deviation from the
reference cache-only sweep, escalated here and in the PR, not silent. Rationale:
a cache-only seam would be an engine-api vendor edit (re-applied after every
upstream sync); the bounded fetch keeps the engine boundary clean and the slice
self-contained. The cache-only parity (a `cached_message_attachments` seam) is a
named follow-up if the owner wants reference fidelity.

## The aggregation

`attachment_browser(filter)` runs per account handle (the search service's
fan-out pattern): `search_mail("has_attachment:true", 200)`, the hits resolved
through `mail_by_keys` newest first, then `message_attachments` per message, the
downloadable parts flattened beside the source row's metadata (account, key,
subject, from, sent_at). The scan cap is 200 messages per account (the warm
pass's work limit; kylins uses 500, the smaller bound is stated). The sender
filter narrows before the fetch (a message the filter drops never dials); the
mime filter narrows the parts a listed message contributes.

Per-message failure logs and continues (best-effort): a source that refuses or a
message that no longer resolves is a skip, never fatal to the browse. Per-account
failure (no provider to read through, a refused query or resolve) is an honest
error entry naming the account. The answer carries the rows newest first across
accounts beside a coverage report: scanned (the flagged messages the store
resolved), indexed (the messages whose parts listed), skipped (the reads that
refused), the failed accounts, and the scan window.

### The has_attachment guess

The `has_attachment` flag is the provider's guess until the source settles. A
true-guess with no parts contributes zero rows (the source reads, the parts are
none, the message counts indexed); a false-guess carrying parts is missed by the
query. The engine settles the flag when a source is cached (the warm pass or the
browse's own fetch-on-miss), so a once-read message's flag is the truth; the
caveat stands for mail never opened. The coverage report states the window so a
bounded answer never claims the whole mailbox.

## The view-model and the command

The view-model (`astra-viewmodel::attachment_browser`) carries the entry: the
part metadata (id `u32`, file_name, media_type, size) beside the source reference
(`MessageKey`, account and key) and the source row's subject, from, and sent_at,
plus the coverage report. Newtypes cross the boundary; the part id stays the
message-scoped `u32` the save command takes. The desktop command lives in
`commands/attachment_browser.rs` (`commands.rs` is at the cap) and pulls through a
new `AppPort` method; a blank filter leg crosses as absent, never a match-nothing
string. The web wire types and wrapper ride `services/attachments.ts` (never
`ipc.ts`, at the cap).

Save As reuses the existing `save_attachment` command verbatim: a browser row
carries the account, key, and part id it names. Open-the-source-message dispatches
`intents.openMessage` and closes the modal (the notification-history precedent;
additive over kylins v1, which has no such action).

## The modal and the entry

A transient overlay copying the shortcuts help pattern: a zustand `isOpen` gate,
null while closed, `useDialogFocus`, Escape and backdrop close, `role="dialog"`
`aria-modal`. Rows group by sender (newest-first groups, the count and total size
beside each). The mime chips (All plus the top buckets by count, every image
collapsed to Images and `application/pdf` to PDF) derive from a mime-unfiltered
snapshot, so an active filter never collapses the chips; the mime narrowing runs
client-side over that snapshot. A sender prefilter banner carries Clear; the
coverage footer states the indexed count, the skips, the scan window, and the
failed accounts beside Refresh; the empty state tells the reader to open messages
so their sources land in the cache. Preview stays a placeholder (the reading
pane's precedent; a bytes wire is a follow-up). The entry is a palette command
(the saved-searches shape); the store accepts a sender filter so a ContactDetail
entry is cheap to add later.

## Known gaps

- **Cache-only reference parity** (a `cached_message_attachments` engine seam) is
  the escalated deviation's follow-up; this slice fetches on miss, bounded.
- **The has_attachment guess**: a false-guess carrying parts is missed until the
  source settles.
- **No persistent index**: the browse recomputes per open (kylins persists until
  clear).
- **Preview** stays a placeholder; a bytes-returning preview wire is a follow-up.
- **No ContactDetail sender-prefilter entry** (the store accepts a sender filter),
  no date-bounds UI, and no Clear-all action.
- **A CLI attachments list verb** is a follow-up (the read-only messages
  list/search shape); no MCP tool is needed for v1.
