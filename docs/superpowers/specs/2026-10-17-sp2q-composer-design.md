# SP2q design: the composer finishing

**Status.** Decided 2026-10-20. Slices branch from main at this commit.

**Scope.** What the backlog names and what remains of it: the formatting
toolbar's missing half (font size, text colour, highlight; bold, italic,
underline, lists, and undo already ship), links finished (edit and remove
in place, typed addresses become links, the scheme check surfaced),
signatures end to end (the core's unused Signature block wired to a
settings editor, per-account assignment, seeding, and the auto-swap with
From), the From picker with an app-level default send account, picture
paste into the message (completing "inline and attached files"), and the
mailto: entry (the core's parser finally consumed). Attach and drag landed
with SP2p and are not re-scoped.

## D1: the formatting completion

The wire already carries font size (four steps), text colour, and
highlight; the editor treats such runs as retained content no one can
edit. The toolbar gains the three controls (a size cycle, two colour
pickers from the app's palette), the editor's marks grow to match, and
the document wire's editable-run rule widens to admit the three marks
both ways: a run formatted in the composer round trips, and a run another
client formatted becomes editable rather than frozen. The toolbar chrome
speaks the app's language from the shared catalog.

## D2: links finished

The link button grows into an in-place popover: the target and the
display text side by side, set and remove on the selection. A typed
address becomes a link when the words leave the caret (space or enter
commits a delimited http, https, mailto, or www prefix as a link run; the
recogniser is client-side and conservative). The scheme allowlist stays
core-side and absolute (http, https, mailto, the length bound); the
composer validates the document's targets before the send and names any
that degrade (a sentence under the toolbar, never a blocked send: the
core's degradation to plain text remains the send's own floor).

## D3: signatures

The core's Signature block finally has a producer. Settings gains the
signature editor: rich text (the same editor marks D1 widened), written
once, assigned per account (one signature an account seeds new mail
with, one it seeds replies with; the same one may serve both). The
composer seeds the block on open (new mail above nothing, replies below
the quote), and the From picker's change swaps a seeded signature the
person has not touched (an edited signature is the person's and stays).
The send renders what the document carries (the renderer already emits
the block; the plain leg carries the RFC 3676 delimiter). An inline
logo does not ride this slice (a picture in a signature needs the
attachment binding at seed time); it is recorded as the row's named
shortfall. signatures.md is created with the zh copy.

## D4: the From picker and the default send account

The shell's account rows gain the sending address beside the display
name. The From field becomes a picker: the account list, the default
first, the choice changing the seeded signature per D3's rule. Settings
gains the app-level default send account (a preference; the new-message
button and the mailto: entry open it, falling back to the first account
when unset). A composition opened from a request (reply, forward) keeps
the request's account and never re-picks.

## D5: picture paste, and the mailto: entry

Paste into the editor stages a picture exactly as a drop does (the
inline disposition, the cid binding, the removable row), completing the
inline-and-attached-files arc; pasted text is untouched. The mailto:
entry: the host's deep-link listener (and the single-instance argument
path) hands a mailto URL to the app, the core's parser turns it into the
prefill (the five fields it honours, everything else dropped, exactly as
RFC 6068 says a client may), and the composer opens prefilled from the
default send account. A mailto: link clicked in the reading pane opens
the composer the same way instead of the OS mail app.

## Known gaps recorded, not hidden

- Tables do not ride (the row names row and column editing; the wire
  carries Table blocks as retained content and the editor freezes them).
- The resize handle does not ride (a pasted or quoted picture keeps its
  size; the wire's width_px rides the block, not a gesture).
- The inline logo does not ride D3 (the attachment binding at seed time
  is its own change).
- The typed-address recogniser honours only delimited ascii prefixes; a
  bare domain never becomes a link.
- The scheme check's surfacing is a sentence, never a blocked send: the
  core's degradation remains the floor.

## Test-first

Every behaviour lands with its red: the wire's round trip for the three
marks, the recogniser's commits and refusals, the signature seeding and
the untouched-swap rule, the picker's account list and the default
preference, the paste staging, the mailto parse-to-prefill (the parser's
tests exist; the host seam is pinned offline), and T4's live proofs: a
formatted send round trips over the harness (the received body carries
the size and colour), a signature-bearing send round trips, and a link
round trips including the refused scheme degrading to plain text.
