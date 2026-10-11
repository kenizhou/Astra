# Invitations: the Astra Mail contract

**Scope.** What happens from the moment an invitation mail arrives: the card
above the reading body, the two conditions that decide it appears at all, the
stored copy no RFC moves onto the calendar, the answer and the routes it
rides, the holds and the staleness, and the emailed-reply fallback with its
question. Binding on every surface that ships a reading pane or a calendar.
Ported in substance from mail-calendar's `invitations.md`, scoped to what
Astra Mail ships today; the gaps are recorded at the end, never left silent.

**Principle.** *An invitation is a meeting someone else owns.* The client
never invents the meeting, never answers twice, and never lets the user
believe an answer was delivered when it was not.

## The two-condition gate

A message renders an invitation card only when both hold: the message carries
an iTIP scheduling part the engine's hardened parser accepts (a `text/calendar`
body with a `METHOD` the parser reads; an object without one is not a
scheduling object), **and** this account is an attendee the invitation names,
matched through the addresses the message was delivered to (`Delivered-To` and
friends), so an alias needs no configuration. The organiser's own copy of a
meeting renders no card.

The gate sorts what it acts on into four kinds:

| Kind | Meaning | Actions |
|---|---|---|
| `rsvp` | A reply is owed and this account owes it | Accept, maybe, decline |
| `cancelled` | The organiser cancelled the meeting | None |
| `informational` | The details, no response owed by this account | None |
| `superseded` | The calendar holds a newer revision of the meeting | None |

The card carries the kind, the account's current participation, the meeting's
title, and the organiser's address; a superseded card says it is out of date
and stops offering an answer.

## The stored copy

No RFC moves an invitation into the calendar; the client stores it, from the
invitation's own bytes, under a key tied to the source message (RFC 6638
§3.2.2's stored-copy model). A server that schedules deposits its own copy and
the stored one follows the server's; an email-only invitation is put on the
calendar when answered.

Unanswered meetings draw as holds, distinct from accepted ones, on the grid,
the month, and the agenda. A declined meeting leaves the calendar; the
invitation mail is the way back.

## Staleness

The stored copy's `SEQUENCE` against the message's decides freshness: a
message whose sequence the calendar already outranks is superseded, and the
card says so instead of offering an answer that would rewrite the newer state.

## The answer and its routes

`RespondToInvitation` answers accept, maybe, or decline, optionally with a
note that becomes the reply's `COMMENT`. The route the answer rides is decided
from the account's calendar capabilities, never assumed:

- **The server schedules** (RFC 6638): storing the answer *is* sending it. The
  receipt reports nothing about the reply's delivery, and no second reply of
  the client's own is sent, because the organiser would hear twice.
- **The server will not send it**: the answer is stored and an emailed iMIP
  reply is submitted through the engine's durable outbox, exactly as a sent
  message: queued before the server is asked, never sent twice, withdrawn with
  the answer's failure if it cannot go.

## The reply prompt

When the emailed fallback's submission fails, or a server that advertised
scheduling reports the reply undelivered, the user is told in plain language
and offered the email instead. The prompt names the meeting and the organiser
(the recipient the email would reach), shows the answer it would carry and the
note given at RSVP, and offers Send or Don't send. The choice can be
remembered per account: Always mails every fallback reply without asking,
Never stays silent, and the default is Ask. A remembered choice routes every
later failure through the same table; nothing stale is carried, the reply is
re-derived from the store at send time.

## Known gaps

- **The card shows no attendee roll-call, conflicts, or that day's grid**; it
  carries the kind, the participation, the title, and the organiser.
- **The invitation's own .ics part** is not yet suppressed from the attachment
  list; a genuinely attached calendar file still shows.
- **The desktop host dials no calendar leg yet** (the calendar contract's
  first gap), so on the shipped host the card's calendar-side state reads
  through whatever the account's providers hold.
