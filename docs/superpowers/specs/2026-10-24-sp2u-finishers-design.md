# SP2u design: the calendar and contacts finishers

**Status.** Decided 2026-10-24. Slices branch from main at this commit.

**Scope.** The scattered finisher rows the backlog names: the attendee
list with responses (detail and read-only editor), the full repeating
rule read-back, the multi-day banner shape with its per-day overflow,
the persisted calendar shape restored on launch, and the sender avatar
on the mail list row. Every engine leg exists (participants carried,
the rule carried, the photo cached); every finisher is a crossing and a
render.

## D1: the attendee list

The event detail gains the roll-call: every participant with a name or
address, their role word where it is not the plain attendee, and their
response state, read from the event's own participants over the
existing detail-read seam. The organiser line stands first (the
owner-role participant, the same source the briefs and the invitation
card read). The editor carries the same list read-only (an editor
cannot change a guest list this slice; the list is there so the person
can see who they are writing to). The snapshot's detail read grows the
participants; the occurrence row stays lean.

## D2: the full rule read-back

The repeating rule crosses to the detail read and renders as a full
sentence built by an app-side human formatter (the house pattern: the
engine carries the rule, the app words it): the frequency with its
interval ("every third week"), the day set where the rule names one,
the end in count or date words, and the honest refusal shape for a
rule the formatter cannot word (the raw interval and frequency, never
a wrong sentence). The editor's create select stays the editable
vocabulary; the read-back is the read. The minimal sentence retires.

## D3: the multi-day banner

The all-day lane grows the banner shape: a multi-day all-day occurrence
renders as one spanning banner across its days (the lane's columns
joined), not a chip per day, with the continuation into hidden days
marked where the window clips it. The per-day overflow count that
expands ships on the month grid (the existing "N more" becoming a
banner-aware count over all-day and timed alike). The timed multi-day
block keeps its clamped shape (a banner is an all-day concept).

## D4: the persisted shape

The calendar's shape (the view kind and the anchor date) persists as a
calendar preference and restores on launch: the app seeds the state
from the preference at boot, the navigation writes it, and a refusal
keeps the standing shape. The horizon's persisted setting is already
its own row and stays. The periodic-refresh and offline-at-launch rows
(78, 79) stay open: this slice persists what the person left, not how
the diary fills.

## D5: the sender avatar on the mail row

The mail list row grows the avatar column: the existing Avatar
component (the monogram, the address-book photo through the validated
cached-photo command) at the row's leading edge, resolved per address
through the existing per-address cache, the column hidden when the
density or width demands it (the row's existing responsive rules). The
photo never blocks the row (the monogram stands until the photo
lands). avatars.md is created with the zh copy, owning the monogram,
the photo, the cache, and the cost note (the per-mount read).

## Known gaps recorded, not hidden

- The attendee list is read-only; editing a guest list is a later
  row.
- The roll-call caps at the first eight participants and folds the rest
  into a count; an expansion affordance (seeing attendee nine of thirty)
  is a later row.
- The rule read-back words the supported vocabulary; an exotic rule
  falls back to the honest interval-and-frequency shape.
- The banner spans the all-day lane only; timed multi-day keeps its
  clamped block.
- Rows 78 and 79 (the offline diary at launch, the periodic refresh)
  stay open.
- The avatar's per-mount read cost stands (contacts.md's note); a
  batched resolve is a later row.

## Test-first

Every behaviour lands with its red: the participants crossing and the
roll-call's ordering/role/response words, the rule formatter's
sentences and its honest fallback, the banner geometry over the lane
(the span, the clip marks, the overflow count), the shape's persistence
round trip and restore-on-boot, the avatar column's presence and its
responsive hiding; and the live proofs: a seeded event with attendees
and a rule rendering both over the harness, and a seeded contact's
photo standing on the sender's row.
