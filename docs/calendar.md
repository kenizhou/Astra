# Calendar: the Astra Mail contract

**Scope.** What the calendar surface carries, how the intent family reads and
writes the diary through the engine, how one occurrence of a series is told
apart from the series, what a drag means on the wall clock, how writes are
gated and reconciled, and what the meeting brief may and may not do. Binding
on every surface that ships a calendar view. Ported in substance from
mail-calendar's `calendar.md`, scoped to what Astra Mail ships today; the gaps
are recorded at the end, never left silent.

**Principle.** *A calendar is a diary the server owns and the client
renders.* Every write is the engine's patch verb under an idempotency key,
every read is the store's materialised truth, and the client never invents an
occurrence the rule did not produce.

## The surface and the snapshot

`Surface::Calendar` carries a `CalendarSnapshot`: the calendar manager set
(each calendar's name, colour, visibility, `can_write`, default marker), the
visible range's occurrence rows, the write states, the reply prompts the
invitation machine owes, and the now-line data (the instant the snapshot was
built at, and the IANA zone the grids lay their days out in). The snapshot is
published before it is signalled: a surface pull never observes a state the
store does not hold.

An occurrence row carries the event's account and key, the calendar, the
title, the resolved UTC start and end, the all-day and recurrence markers, the
occurrence token (below), the participation, whether the row may move, and
the series-edit warning. Times are occurrence-accurate: each row of a series
carries its own instants, resolved through the expansion zone the horizon was
widened in. The snapshot also publishes the calendar's standing shape (the
view kind and the navigation anchor, below), so a surface restores what the
person left.

## The event detail read

Picking an event pulls its detail over the host command
`calendar_event_detail`: the roll-call and the full repeating rule, both read
from the master the event reference names through the same targeted read the
rebuild rides. The occurrence row stays lean; this read is the only carrier
of the two halves a crowded meeting and a rich rule need.

- **The roll-call** carries the owner-role participant (the organiser) first,
  every further participant behind in stored order, each as one line: the
  name, or the address when the line carries no name, the word for a seat
  beside the plain attendee's (`chair`, `optional`, `informational`,
  `contact`, a foreign role verbatim), and the response state. The list caps
  at the first eight lines and folds the tail into a count the surface
  renders as the honest end of a crowded meeting; an expansion affordance is
  a later row.
- **The rule read-back** crosses the rule's own shape beside the sentence
  the app words from it (the house pattern: the engine carries the rule, the
  app words it): the frequency with its interval, the day set where the rule
  names one, and the end in count or date words. A rule outside the wording
  vocabulary crosses only what the surviving parts still make true, down to
  the bare interval and frequency, never a sentence the rule does not make
  true. The until clock words its seconds whenever they are not zero, so a
  bound inside a minute never reads as the whole minute nor as end of day.
- The editor carries both halves **read-only**: an editor cannot change a
  guest list or reword a rule this slice; the list is there so the person
  can see who they are writing to, and the sentence is the read where the
  create select stays the editable vocabulary.

## The persisted shape

The calendar's shape (the view kind and the navigation anchor) persists as a
preference and restores on launch. A `set_range` that names its view records
it; `set_date` records the anchor; a write that names neither leaves the
standing preference untouched, and a refused write keeps the standing shape
(the snapshot publishes it on every rebuild, and a surface follows the
publish). A past anchor restores exactly: the boot seeds the window around
the anchor's own week, so the diary paints on the day the person left, not
today. The horizon's persisted setting is its own row and stays.

## The all-day banner

A multi-day all-day occurrence renders as **one** spanning banner across its
days in the all-day lane, the columns joined, with the continuation into
days the window hides marked at the clipped edge; banners that share days
stack in their own lanes. A timed occurrence, however many days it spans,
never becomes a banner: it keeps its clamped block, because a banner is an
all-day concept. The month grid chips a spanning all-day occurrence once, on
the day it starts, so its overflow count names occurrences rather than
repeats.

## The window and the horizon

The diary renders a window the surface sets (`SetRange`), widened by the
engine's occurrence expansion in chunks. The watermark that records what has
been expanded is **shared across accounts**: a widening advances it only after
**every** account's store has expanded to cover the span, and any account's
refusal rolls the watermark back. A store left wider than the watermark is
safe, because occurrence reads are windowed by the caller; a watermark left
wider than the stores is not, because a later range inside it would render
confidently empty and no sync repairs that. A refused widening is a failure,
answered as such, never a confident empty.

## The intent family

`Intent::Calendar` carries the family, serde snake_case, additive:

| Intent | Meaning |
|---|---|
| `refresh` | Sync the window's calendars and republish |
| `set_range` | Set the visible window (start, days, maybe the view it serves) |
| `set_visible_calendars` | Toggle calendar visibility for the rebuild |
| `create_event` | The engine's draft, on a calendar the row says is writable |
| `update_event` | The patch verb: title, notes, location, clocks, rule |
| `move_event` | A drag: which edges, whole days, minutes, maybe an occurrence |
| `delete_event` | The series, or one occurrence the token names |
| `respond_to_invitation` | The RSVP (the invitations contract) |
| `set_date` | The navigation anchor (today, a day picked) |
| `answer_reply_prompt` | The emailed-reply fallback's question, answered |

## The occurrence token

One occurrence of a recurring event is named by its **original start** as a
wall clock in the event's own zone. The token is deliberately opaque to the
surface: an identifier for "the Tuesday you dragged", not a time to compute
with. A write that names an occurrence first proves the token is one the
snapshot minted, by re-minting over the store's window; a stale token is
refused typed and writes nothing, because splitting a series at a time the
rule never produced corrupts it.

## One or all

Series or one occurrence is a question for the user, never a default: the edit
and delete paths ask before they cross, and the write carries the answer.

An occurrence-scoped edit states its clocks. The engine's CalDAV path refuses
a first-touch override without both, because the copy would inherit the
master's slot (the master's times are the *first* occurrence's). The rules:

- **A host that states neither clock is making a content-only edit.** The app
  fills both unchanged from the stored event, whose zone only it holds.
- **A host that states one clock without the other is refused**, typed:
  guessing the missing half would re-time the occurrence.
- **A move states both**, and a drag is converted to them.

## The drag currency

A drag is whole days and minutes on the wall clock, applied in the event's
**own** zone: the delta is exact for floating and all-day events, and for a
zoned event equals the drag semantics a grid expresses. A later occurrence
moves on **its own** clock, not the master's; the override the engine writes
takes its new times from the patch, so shifting the master's bounds for a
later occurrence would land the block in the series' opening slot.

Resizes clamp at a quarter-hour floor (a whole day when all-day); the day
component of an all-day drag may carry minutes that are dropped rather than
rounded onto a neighbouring date. A drag beyond 366 days is refused rather
than converted, because the civil-date arithmetic saturates outside
representable calendar time.

## The series warning

Editing a whole series where the server would discard per-occurrence edits
warns before the write crosses. The warning fires on exactly two facts, both
required: the row's override survival says the server discards per-occurrence
changes, **and** overrides are present to discard. Neither fact alone warns.

## Write gating and write states

Every write consults `can_write` on the row's calendar before the engine is
touched; a read-only calendar offers no write affordances and refuses the
write typed. Writes surface their state per account: `writing`, then settled,
or `failed` with retry. A failed write republishes the store's truth; the
surface never asserts an unconfirmed move after a refusal. An optimistic grid
move is cleared by the next publish, whichever way it settles.

## Meeting briefs

The day's meeting brief is generated on request (the host command
`calendar_brief_generate`, the app's `generate_meeting_brief`) on the Economy
tier, resolved through the use-case registry (never a literal at the call
site), with one correlation id minted before the call and recorded in the
ledger row. The meetings are re-read from the store, never a cached snapshot
field; the day's horizon is widened first, and a refused widening fails the
brief rather than answering a confident no-meetings. Meeting content rides
the redaction path before the untrusted wrap; timings stay trusted. A day
with no meetings after the read answers the defined no-meetings state and
spends **no** call. Briefs regenerate on request; nothing is cached.

## The live proofs

The gated harness suite proves the diary live, seeded straight onto the
calendar face under unique run tokens and read back through the product's own
sync and snapshot: the diary sync materialises the seeded plain, weekly, and
all-day events with occurrence-accurate times, and the write family
round-trips (a create, a title edit, a drag move, the content-only occurrence
edit that fills both clocks unchanged, and deletes), every write state
settling by the time its dispatch returns. The invitation machine's proof
walks the whole flow over the scheduling route the harness serves, and the
meeting brief's proof generates from the stored diary through the relayed
model tier. The finishers' proof seeds one event carrying a full guest list
(an organiser merged with her own chair line, attendees across the role and
response seats, two more lines than the roll-call's cap) and a weekly rule
with a day set and a count bound, then reads both back through the detail
read: the organiser first with the role and response words, the folded count,
and the sentence that words the rule in full. Every proof withdraws what it
seeded, so the harness keeps the shape the mail suites pin.

## Known gaps

- **Edit-time rule restructuring is absent.** The create path offers the
  repeat choices; the edit path degrades read-only rather than rewrite a rule
  it cannot read back in full.
- **The visible window moves by control only.** Toolbar arrows, Today, the
  view switch, and month day-cells set the range; scrolling a grid does not.
- **Grid drags move whole blocks.** A resize is offered by the event dialog's
  end edit, not by a grid edge handle.
- **The roll-call caps at eight lines with no expansion affordance.** A
  meeting of thirty shows eight names and the folded count; seeing attendee
  nine is a later row.
- **The rule read-back words the supported vocabulary.** An exotic rule
  falls back to the honest interval-and-frequency shape rather than a wrong
  sentence.
- **No periodic refresh timer; the offline diary at launch is a later row.**
  The shape the person left now restores on launch; entry and focus regain
  refresh, and rows 78 and 79 stay open.
- **Briefs are not persisted.** Each request regenerates.
