# SP2s design: the reading batch

**Status.** Decided 2026-10-24. Slices branch from main at this commit.

**Scope.** The reading batch the backlog names: the message own-window,
Save as .eml, print, reading zoom and reflow, spam reporting, and swipe
actions. The engine ships every verb (the `.eml` source read, the report
machinery all four adapters implement); the composer's pop-out window is
the own-window's precedent; the attachment save dialog is the `.eml`
save's. Today every one of these is a placeholder row.

## D1: the message own-window

A message opens in its own OS window following the composer window's
pattern exactly: a URL-param-mounted root in the same bundle, a minted
window id, the capability's window wildcard, the closing semantics (the
window closes when its message closes). The window carries the whole
reading view the pane renders (header, invitation card, AI surfaces,
body, attachments, the reply strip) over its own snapshot pull; opening
from the row menu moves nothing (the pane keeps its own open message
until the person changes it); the pane and the window are two readers of
one store, and a mark-read from either lands once. reading-window.md is
created with the zh copy.

## D2: Save as .eml and print

Save as .eml writes the engine's `message_source` bytes (never a
re-serialization) through the SP2p save-dialog seam, the suggested name
derived from the subject. Print renders a print-optimised document (the
subject and the header lines the row names, the sanitised body at the
reading zoom, attachments as names) and opens the platform print dialog
over it; the reading pane's overflow menu gains both verbs beside the
existing ones.

## D3: reading zoom and reflow

The reading body gains a zoom control (the pane's header: smaller,
default, larger, tracked per reader in the local appearance store beside
the font-size step). The zoom reaches INSIDE the sanitised iframe: the
srcdoc is wrapped in a document whose base font-size scales and whose
viewport meta the sender's mobile rules read, so a newsletter laid out
against the pane's own width applies its own rules at every width (row
40). Fixed-width messages (a table or a hard pixel width) reflow to fit:
the wrapper clamps the content width to the viewport and the body text
reflows live during resize (row 41). The plain-text leg scales with the
same steps. reading-zoom.md is created with the zh copy.

## D4: spam reporting

The row menu's Junk placeholder becomes real: a Mark-as-spam verb that
reports the message to the provider through the engine's outbox-mediated
report (the verdict Junk; the provider files or flags per its own
machinery) and moves it to the Junk folder (a report and a move are two
acts; the person expects both). The reading pane's verb row gains Not
spam (the NotJunk verdict) for a message already in Junk, which files it
back to the Inbox. The undo question: a report rides the outbox and the
move is the ordinary move-to-folder both the row menu and the swipe
already offer; the capability's evidence note (Convention for most
providers) words the row honestly. reporting.md is created with the zh
copy.

## D5: swipe actions

The message list's rows gain touch swipes: a horizontal pointer gesture
on a row reveals the action behind it (right swipe the primary action,
left the secondary), the defaults Archive and Trash, configurable per
direction in the settings' Reading section (the settings row names the
absence today), with an undo toast that reverses the move. The swipe
never fires on the drag channel (the folder drag owns the mouse; the
swipe owns the touch pointer), and the virtualised list's scroll keeps
its own axis.

## Known gaps recorded, not hidden

- The own-window is one message at a time per window; a multi-message
  window is not this slice.
- Print prints the sanitised reading view, never the raw source; a
  "print original" is not offered.
- The zoom's steps are three, matching the app font steps; a free
  percentage is not offered.
- The phishing verdict (the third of the engine's) has no surface; the
  engine exposes it for a later row.
- Swipe actions are touch-only; the trackpad gesture story is the
  platform batch's.

## Test-first

Every behaviour lands with its red: the own-window's open/close and the
shared mark-read; the `.eml` save's byte honesty (the engine's source,
never re-serialized) and the dialog outcomes; the print document's shape
(subject, headers, body at zoom); the zoom's reach into the wrapper and
the reflow clamp; the report's verdict/filing/idempotency over fakes and
the Not-spam return; the swipe's direction config, the undo, and the
non-interference with scroll and drag; and the live proofs: a `.eml`
exported over the harness matching the received source, a junk report
landed on the real server (the keyword/filing observed), and a
fixed-width body reflowing at two pane widths.
