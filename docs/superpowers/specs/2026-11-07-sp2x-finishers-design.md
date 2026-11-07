# SP2x design: the surface finishers

**Status.** Decided 2026-11-07. Slices branch from main at this commit.

**Scope.** The six surface rows the matrix still holds unstarted: the
Drafts row reopening into the composer, the image resize gesture, the
per-row permanent delete, the boot calendar fetch, the periodic calendar
refresh, and the showcase flag. Every engine verb the rows need already
sits in the vendored tree; this slice is app-layer wiring and gestures.

## D1: the permanent delete

One verb end to end: a `delete_message` core function over
`MailEdit::delete` through the same outbox-mediated edit path the other
write verbs ride, an intent, and a confirm-gated row-menu item. The
sentence copies the Trash folder delete's own shape ("This cannot be
undoed.") and the item stands anywhere the menu opens, danger-styled,
below the ordinary Delete (which stays the move-to-trash). The Trash's
bulk delete keeps its own surface.

## D2: the boot calendar fetch

The diary fills without opening it: the boot's per-account first-sync
spawn gains the calendar fetch, joined beside the mail sync so an
account that connects also refreshes its calendar once. The refresh
body is called directly, never the intent (a boot-time intent would
republish the calendar surface needlessly); the paint half already
ships (the store prime at boot), so a previously synced calendar paints
at launch and the fetch fills whatever changed while the app was away.
An account added at any time gets the same join.

## D3: the periodic calendar refresh

A host timer beside the delivery loop: a five-minute tick, gated on at
least one account having connected (the dials non-empty), calling the
non-intent refresh body so the surface signal fires only on change. The
interval is debug-overridable (an env override with a limit for
deterministic tests, the reference's own shape). The reference's known
gap is now ours to close: the standing cadence while the app stays open.

## D4: the Drafts row reopens

A draft row opens back into the composer over the same copy: a by-key
`resume_draft` beside the latest-entry (the message reference names the
account and key; the three refusal gates port — unresolvable, not a
draft, body unreadable), the resume carrying the draft's attachments as
pre-attached removable rows in the forward's own shape (the carried
parts ride the core-side part-id carry; the bytes never cross the host
boundary twice), and the next save writing over the same key and
`Message-ID` (the adoption machinery already keyed so). The two row-menu
verbs land: Open (the resume into the pane) and Delete Draft (the
discard machinery over the row's key, outbox-queued on failure).

## D5: the image resize

A composer-inserted picture renders (today it stages as a text row with
a pill): the inline image renders beside the editor as a visible
picture (the retained-block separation keeps the chrome out of the
message), and the resize gesture ports — pointer events on a fixed
overlay outside the content, corner grips, the dragged width clamped
(24px floor, the available width ceiling), the height following only
where the tag already declares one, and only the WIDTH travelling on
the wire into the image inline's `width_px`. Quoted originals keep
their width/height attributes (the sanitiser preserves them); the
gesture covers composer-inserted pictures this slice, and the quoted
originals' gesture is recorded as the row's named remainder (the quote
HTML is not rendered today).

## D6: the showcase flag

A debug-gated launch flag boots the fixture-backed shell: the desktop
host's command layer answers the recorded snapshots when the flag
stands, so the production webview bundle runs unmodified against
fixture answers (the alias rule keeping the harness out of production
stays intact). The gate is debug-build-only; the screen picker (which
surface stands: list, reading, settings, people) ports as an env; the
snapshots' dates stand as recorded (a frozen clock, the `seeded_now`
idea approximated). The reference's in-memory provider boot is recorded
as not-taken with the reason (unvendored bindings code against a
dial-first boot; a screenshot run does not need real-engine fidelity).

## Known gaps recorded, not hidden

- The quoted originals' resize gesture rides a later slice (the quote
  HTML is not rendered today).
- The showcase fixtures are static records with no pinned wall clock;
  the dates are frozen as recorded.
- The periodic refresh cadence is fixed at five minutes (no setting).
- The permanent delete's confirm is the row menu's own; no undo stands
  behind it (the engine's verb is irreversible).

## Test-first

Every behaviour lands with its red: the delete verb over the fakes and
its confirm-gated menu item; the boot fetch join (the calendar fills
without the view opening, the surface signal silent); the periodic tick
(gated, debug-overridable, the change-only signal); the by-key resume
(the three gates, the carried rows, the same-key save); the rendered
picture and the gesture (the clamped width, the ratio rule, the
wire carrying only the width); the showcase flag (the gate, the screen
picker, the unmodified bundle); and the live proofs: the delete verb
observed over the harness (the copy truly gone), the boot fetch filling
the calendar over the harness, and the drafts reopen round trip (a
draft saved, the row opened, the save landing over the same key).
