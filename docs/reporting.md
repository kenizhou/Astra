# Spam reporting: the Astra Mail contract

**Scope.** Reporting mail as spam or not spam, over the engine's
outbox-mediated report, on every surface that offers the verbs. The gaps are
recorded at the end, never left silent.

**Principle.** *A report is one act.* The engine's report carries its own
filing: the caller resolves the destination exactly as it resolves Trash for
a delete (the account's Junk folder for a spam report, the Inbox for a
not-junk correction), and the provider files the message as part of
reporting. No second move rides beside it on any family: the IMAP, JMAP,
Graph, and Gmail adapters all file through the report itself.

## The verbs

- **Mark as spam** stands in the row menu over any row the account can
  report: one durable report per member message, the Junk verdict, filing
  into the account's Junk folder (the RFC 6154 role first, a conventional
  folder name otherwise: junk, junk mail, spam, and their kin).
- **Not spam** stands in the reading pane's verb row when the message was
  opened from the account's junk folder and the account can report: the
  NotJunk verdict, filing back into the Inbox. The browsing context (the
  folder the list sat in when the message opened) is what the surface
  judges, so the verb never stands over a message merely sitting somewhere
  else.

Both ride the engine's durable outbox, one idempotency key per report, so a
spam report and its later correction stay two ops and neither is lost to a
closed app. The outcome is deliberately not a return value: the report rides
the outbox and the republished list is what tells the reader.

## The verdicts and the evidence

The engine holds three verdicts; the surface ships two. **Junk** and
**NotJunk** are the verbs above. **Phishing** (a deliberate attempt to steal
or impersonate, a stronger claim the transports route differently) has no
surface: the engine exposes it for a later row.

What the provider actually reports back differs by transport, and no surface
claims more than the evidence class holds:

- **Acknowledged**: the provider takes the report as an explicit action and
  answers whether it was accepted. Graph only.
- **Convention**: the report is a flag or a label the provider is expected
  to notice (the IMAP `$junk` keyword, JMAP's `$junk`, Gmail's SPAM label),
  and the protocol offers no way to tell whether it did. This is not the
  claim that it probably did not work; it says only that the engine holds
  no evidence, and neither does anything built on it.

## The visibility rule

A report verb stands only where the account's provider can report mail AND
the account's tree resolves a junk destination (the role or a conventional
name). Where either fails, the verb is not offered at all, never offered and
then refused: dispatching it could only be skipped.

## The undo story

There is none, and that is the honest answer: a report rides the outbox and
carries its own filing, so there is no separate move to undo. The remedy is
the opposite verb: Not spam files the message back to the Inbox, as its own
report.

## Known gaps

- **The phishing verdict has no surface.** The engine exposes it; no verb
  offers it.
- **No bulk not-junk.** The correction stands in the reading pane's verb row
  over one message; the row menu carries no Not-spam batch.
