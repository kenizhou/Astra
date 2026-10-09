# SP2e Outbox and Sending Design

Date: 2026-10-09
Status: proposed
Parent spec: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (sections 5, 15)
Hard prerequisite carried: the SP2d final review's then_close gate fix lands here, paired with switching Close to Save{then_close: true}.

## 1. Purpose

Send mail and never silently lose a send: the new-compose send path through
the engine's durable outbox, the outbox family (show, cancel, send-now, the
unconfirmed-delivery prompts), the Sending surface, SMTP provider binding at
connect, and the flush-on-leave close. Reply and forward (quoting, sender
prefill) arrive with the next slice; this one sends what the composer holds.

## 2. Decisions

- **D1**: `Intent::SubmitRichMail { from, to, cc, bcc, subject, document,
  blobs }` ports in MC's shape (blobs present on the wire, this slice's
  editor produces none; attachments arrive later); it renders through
  astra-composer and submits via engine `submit_mail`, entering the durable
  outbox. The composer's Send button dispatches it with the CURRENT editor
  content (no flush-window gap).
- **D2**: the outbox family ports as `OutboxIntent::{Show, Cancel{
  queued}, SendNow{ queued}, ConfirmSent, ConfirmNotSent}` (MC shapes minus
  Edit, which needs the staging-directory flow of the attachments slice;
  recorded deferral); `Surface::Sending` carries the queue snapshot
  (per-account rows: op id, state Queued | Sending | Unconfirmed | Failed,
  subject line, arrival instant).
- **D3**: close flush: the composer's Close switches to `Save{then_close:
  true}` AND the then_close forget moves under the save gate in
  `close_composition`'s shape (the SP2d prerequisite; both halves land
  together, pinned by the fire-then-close race test extended to the
  then_close path).
- **D4**: the SMTP leg binds at connect: `connect_mail_providers` registers
  the submission provider from the account's `[smtp]` config beside the
  IMAP bindings (MC's provider set), so the engine's outbox drain can
  deliver; the drain runs after every RefreshMail (already wired) and after
  every submit.
- **D5**: the frontend gains: a Send button (dispatches SubmitRichMail then
  closes the pane; on typed error it surfaces and keeps the pane open with
  words intact), an outbox panel (queue rows with cancel and send-now
  actions, unconfirmed rows with the Mark as sent and Send again prompts),
  and a send-result toast on the surface signal.

## 3. Testing

- Unit (astra-app, in-memory engine + fakes with an SMTP leg): submit lands
  a queued op; drain delivers through the fake SMTP; SendNow retries now;
  Cancel withdraws; the unconfirmed path (delivery state unknown) surfaces
  the prompt rows; ConfirmSent and ConfirmNotSent resolve them; the Sending
  surface publishes on every transition; the then_close race pin from SP2d
  extended to the gated forget.
- Wire: SubmitRichMail + the outbox family + the Sending snapshot literals
  pinned both sides.
- Frontend (vitest): Send dispatches with the current document and closes
  on success, stays open with the error on failure; outbox rows render with
  actions dispatching exact payloads; the unconfirmed prompts; the toast.
- Live (gated, harness): submit a mail from astra1 to astra2 through the
  harness SMTP; assert the outbox drains to delivered and astra2's INBOX
  holds the mail after a sync; the unconfirmed path exercised by a fake
  refusal variant in unit tests only.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build and vitest
   green.
2. A mail sent from the composer reaches astra2's INBOX in the live proof;
   cancel and send-now behave; the unconfirmed prompts resolve.
3. The SP2d prerequisite is discharged: Close flushes the final window, and
   the extended race test pins the gated forget on the then_close path.
4. `docs/capabilities.md` rows move (sending partial, outbox partial) with
   the zh mirror, a changelog fragment, and the `docs/sending.md` contract
   doc ported in substance from MC (never-send-twice, unconfirmed
   semantics) in the same change.
5. Additive wire only; existing fixtures and tests pass unchanged.
