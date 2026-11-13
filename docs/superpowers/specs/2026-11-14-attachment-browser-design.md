# Attachment browser design

**Status.** Decided 2026-11-14. Slices branch from main at this commit.

**Scope.** Attachments are only reachable per-message in the reading pane; there
is no cross-message browse. This kylins addition adds an attachment browser: a
transient modal listing attachments across mail, grouped by sender, filtered by
mime, with Save As and open-the-source-message. It is a bounded fan-out over the
engine's existing seams (search has_attachment -> resolve -> message_attachments),
a command returning a list + coverage report (not a new Surface), and a modal
overlay (not a fourth view). No engine change; no eval/pipeline bump.

## Reference deviation, escalated (AGENTS.md)

The locked kylins reference (attachment_index.rs, the attachment-browser feature)
builds a rebuildable SQLite index with a CACHE-ONLY sweep: it reads cached raw
sources and NEVER dials the server (a miss is counted skipped). Astra's only
engine seam (message_attachments) is cache-first but FETCHES on a miss, and
engine-api exposes no cache-only attachment read. This slice ships FETCH-ON-MISS
with a bound (no engine change): the first open warms the scanned messages'
sources over the network, bounded by the scan cap, and the coverage footer states
the window honestly. This is a deliberate deviation from the reference cache-only
sweep, escalated here and in the PR, not silent. Rationale: a cache-only seam
would be an engine-api vendor edit (re-applied after every upstream sync); the
bounded fetch keeps the engine boundary clean and the slice self-contained. The
cache-only parity (a cached_message_attachments seam) is a named follow-up if the
owner wants reference fidelity. The has_attachment flag is the provider's GUESS
until the source settles (a true-guess with no parts contributes zero rows; a
false-guess carrying parts is missed); the coverage caveat is recorded.

## D1: the core aggregation

A new crates/astra-app/src/attachment_browser.rs (lib.rs at the cap, reclaim a few
doc lines), an impl App method attachment_browser(filter) -> AttachmentBrowserView.
Per account handle (the search.rs pattern): search_mail("has_attachment:true",
cap) -> mail_by_keys -> message_attachments per message -> flatten the parts beside
the source row's metadata (account, key, subject, from, sent_at). The scan cap is
200 messages per account (the warm limit; kylins uses 500, the smaller bound is
stated). Per-message failure logs and continues (best-effort); per-account failure
is an honest error entry. The answer carries the rows + a coverage report (scanned
/ indexed / skipped / failed accounts). A sender filter (exact, case-insensitive)
and a mime-prefix filter narrow the rows. No persistent index: Astra recomputes
per open (always fresh; kylins persists until clear, a recorded difference).

## D2: the view-model + the command + the web service

A new crates/astra-viewmodel/src/attachment_browser.rs (re-exported from lib.rs):
the entry = the part metadata (id u32, file_name, media_type, size) beside the
source reference (MessageKey account+key) + subject/from/sent_at, and the coverage
report. Newtypes across the boundary; the part id stays the message-scoped u32 the
save command takes. A new desktop command in commands/attachment_browser.rs
(commands.rs at the cap) pulls through a new AppPort method, registered in lib.rs.
The web wire types + wrapper ride services/attachments.ts (never ipc.ts, at the
cap). Save As reuses the existing save_attachment command verbatim (a browser row
carries account+key+part id); open-the-source-message dispatches intents.openMessage
and closes the modal (the notification-history precedent; additive over kylins v1,
which has no such action).

## D3: the modal + the entry + the contract

A transient overlay (NOT a fourth view; ShellView stays mail/calendar/people, the
rail untouched, the kylins "never a folder" red line). A new
features/attachment-browser/ (AttachmentBrowser.tsx + attachmentBrowserStore.ts +
pure grouping/chip helpers), copying the ShortcutsHelp overlay pattern (zustand
isOpen, null while closed, useDialogFocus, backdrop/Escape close, role=dialog
aria-modal), mounted once in the AppShell tail beside CommandPalette. Rows grouped
by sender (newest-first groups, per-group count + total size), mime chips (All +
top buckets, image/* collapsed to Images, application/pdf to PDF, derived from a
mime-unfiltered snapshot so an active filter never collapses the chips), a sender
prefilter banner with Clear, a coverage footer (indexed from N cached messages, K
not cached, scan window, failed accounts) + Refresh, an empty state telling the
user to open messages so sources land in cache. The entry is a palette command
(the saved-searches shape). Preview stays a placeholder (the reading-pane precedent;
a bytes wire is a follow-up).

The contract: a new docs/attachment-browser.md (the fan-out, the escalated
fetch-on-miss deviation, the has_attachment guess caveat, the scan cap, the
no-persistent-index rule, the recorded gaps) + zh; the capabilities row; the
porting-backlog shipped-list move; a changelog fragment. The zh mirrors move in the
same commit.

## Known gaps recorded, not hidden

- The cache-only reference parity (a cached_message_attachments engine seam) is the
  escalated deviation's follow-up; this slice fetches on miss, bounded.
- The has_attachment guess coverage caveat (false-guesses carrying parts are missed).
- No persistent index (recompute per open; kylins persists until clear).
- Preview stays a placeholder; a bytes-returning preview wire is a follow-up.
- No ContactDetail sender-prefilter entry (the store accepts a sender filter so it
  is cheap to add); no date-bounds UI; no Clear action.
- A CLI attachments list verb is a follow-up (the read-only messages list/search
  shape); no MCP tool needed for v1.

## Test-first

Every behaviour lands with its red. Core aggregation tests over the fake provider
(seed multipart/mixed sources, run the browser method, assert the flattened rows,
source references, newest-first order, the cap, the coverage report, per-account
error honesty, the sender/mime filters). Pure grouping/chip helper table tests. Web
view tests (vitest, act() mid-test, mock the service whole). Desktop command tests
over the fakes + the SaveHost seam. ONE gated live harness proof
(attachment_browser_proof.rs: submit a note carrying an attachment, sync, browse
through the new core method, assert the row + the byte read, withdraw both copies).
No live model proof; the offline battery is the proof.