# SP2h Calendar Design

Date: 2026-10-11
Status: proposed
Parent spec: `docs/superpowers/specs/2026-08-...-workflow-design` (sections 5, 15)
Port contracts: mail-calendar `docs/calendar.md` and `docs/invitations.md` in substance.

## 1. Purpose

Calendars and events: the account's calendars, the agenda and day/work-week/
month grids, event create/edit/move/resize/delete, recurrence (read-back and
one-vs-all occurrence edits), meeting invitations with RSVP and the iMIP
fallback, write gating on can_write, and the meeting briefs returning from
A4's deferral (they were deferred pending calendar data existing).

## 2. Decisions

- **D1 (surface)**: a `Surface::Calendar` carrying the `CalendarSnapshot`
  (the calendar manager set, the visible range's occurrences, write states,
  the now line data). The shell's view switch gains Calendar beside Mail/
  Contacts. The SPA renders the agenda plus day, work-week, and month grids
  (kylins' layout stack adapted; the calendar chrome is Astra's own).
- **D2 (intents)**: the family `Calendar(CalendarIntent)` with `Refresh`,
  `SetRange{start, days}`, `SetVisibleCalendars{ids}`, `CreateEvent{...}`,
  `UpdateEvent{event, edit: EventEdit}`, `MoveEvent{event, drag}`, `DeleteEvent
  {event, occurrence: Option}`, `RespondToInvitation{...}`, `SetDate{date}`
  (the jump-to-today/navigation anchor). The engine's calendar read/write
  surface serves (READ at 68eab00 first: calendars, occurrences_in,
  events_by_keys, submit paths, invitation reply).
- **D3 (recurrence)**: read-back of RRULEs via the engine's occurrence
  expansion; occurrence-scoped edits/deletes (one vs all) with the series
  warning where the server would discard per-occurrence edits; occurrence-
  accurate dates in the grids.
- **D4 (invitations)**: the invitation card above the reading pane body,
  accept/maybe/decline through the engine's reply path; declined meetings
  leave the calendar; unanswered drawn as holds; stale-invitation detection;
  the iMIP email reply fallback where the server cannot deliver RSVPs; the
  reply-delivery-failure prompt (MC's contracts).
- **D5 (meeting briefs)**: `briefings(account, day)` (host-side; ECONOMY
  tier) producing the day's meeting brief over the calendar's events and
  the attendee/message context where linked; the calendar view gains a
  Briefs action. The A4 capabilities row moves.
- **D6 (write gating)**: every calendar write consults can_write; the
  surface carries the write-status states (writing, failed with retry) per
  MC; optimistic grid moves reconcile on sync.

## 3. Testing

- Unit (astra-app, in-memory engine + fakes): the snapshot over ranges;
  occurrence expansion; event CRUD intents mapping to the engine calls;
  drag/move math; the one-vs-all edit split; the invitation state machine
  (RSVP outcomes, the hold drawing, staleness, the fallback); the brief
  service over fixture events; write gating + the failed-retry state.
- Wire: the CalendarIntent family + CalendarSnapshot literals pinned.
- Frontend (vitest): the grids (day/work-week/month, the now line, the
  banners), the agenda, event create/edit dialogs, drag interactions (the
  honest jsdom approximation), the invitation card, the briefs panel.
- Live (gated, harness): Stalwart serves CalDAV (verify); a calendar sync,
  an event round trip (create -> edit -> delete) over CalDAV, an RSVP
  flow, and the brief over the seeded events.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build, vitest,
   sidecar tests green.
2. Offline: the grid/CRUD/recurrence/invitation lifecycles pinned; the
   brief service pinned.
3. Live: a CalDAV round trip against the harness where Stalwart supports
   it; otherwise the honest recorded gap with the engine's supported
   provider path proven.
4. `docs/calendar.md` + `docs/invitations.md` (+ zh) ported in substance,
   capabilities rows moved (calendar, invitations, meeting briefs), the
   fragment.
5. Additive wire only; existing fixtures and tests pass unchanged.
