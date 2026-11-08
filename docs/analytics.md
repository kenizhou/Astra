# Consented analytics: the Astra Mail contract

**Scope.** The product analytics Astra Mail records: the consent
machinery, the payload rule, the local-only sink, and the ledger the
events land in. Binding on every host piece that reports a product
event. No reporting endpoint ships; the gap is recorded at the end,
never left silent.

**Principle.** *Consent is the switch, and the switch is one click
each way.* Off by default, granted by one click, withdrawn by the same
click turned off. Nothing is recorded while it is off, and the
withdrawal stops the flow at the next call, because the gate reads the
persisted preference live on every call, not a state latched at boot.

## The consent gate

The recorder lives in the app core. Every call passes the gate:

- **No consent, no row, no error.** A consent that never stood records
  nothing; the call answers nothing and never fails the host flow it
  rides inside.
- **Consent through the settings intent.** The Privacy settings
  category's switch dispatches the same write every consent verb uses;
  the preference persists before the dispatch answers.
- **The withdrawal is the same write turned off.** One click, and the
  next call records nothing. A consent granted again records again:
  the gate reads the live preference.
- **No ledger installed, nothing recorded.** An app with no ledger
  records nothing, the same shape an app with no account store renders
  no rows with; a ledger write that fails logs and the host's flow
  moves on.

## The payload rule

Counts and shapes only, never content. One event is a shape name from
the host's own closed vocabulary and named integer counts, so no
subject, address, or body can ride a value: the row shape itself keeps
it so, the same rule the tool-call ledger's column set follows. The
shapes that ship today: `app_open` (the boot's own event, carrying the
standing account count), `log_exported` (the byte count), and
`default_mail_registered` (a bare occurrence).

## The local-only sink

The consented events land in the app's own AI ledger, the same sqlite
store (`ai.sqlite`) the tool-call and LLM-call ledgers live in, in its
own `analytics_events` table: one row per event, the shape, the counts
as a JSON object of name-to-integer pairs, and the store's own
creation stamp. The rows tie to no correlation id: these are product
events, not agent turns.

Nothing leaves the device. No endpoint exists, and inventing one is
the owner's call, not a port: the sink and the reporting endpoint are
the row's named wait, and the Privacy statement states the truth in
the house voice (the recorded events are counts and shapes kept on
this device).

## Known gaps

- No sink beyond the local ledger and no reporting endpoint ship: what
  stands is the consent machinery, the no-content payload, and the
  withdrawal.
- No surface reads the ledger back: the rows are the owner's own to
  read from the sqlite file.
