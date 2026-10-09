# SP2e Outbox and Sending Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver sending end to end: SubmitRichMail through the engine's durable outbox with SMTP provider binding, the outbox family and Sending surface, the flush-on-leave close discharging the SP2d prerequisite, the composer Send button and the outbox panel, live delivery proof across the harness.

**Architecture:** astra-app gains the submit arm, the outbox family over engine ops (cancel/retry/confirm semantics), the Sending snapshot, and the gated then_close forget; astra-account binds the SMTP submission provider at connect; the SPA sends from the composer and manages the queue.

**Tech Stack:** Existing workspace; engine submit_mail/outbox ops at the pinned rev; MC's sending contract doc as the semantics source.

**Spec:** `docs/superpowers/specs/2026-10-09-sp2e-outbox-sending-design.md`

## Global Constraints

- House rules as prior slices: SPDX, 500-line cap, deny-everything lints, gate before commit (direct exit, never piped), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- Port roots: MC = D:/Projects/mailclient/opensource/mail-calendar (mailcal-app outbox_ops/send.rs, protocol/intent_outbox.rs, bindings send paths, docs/sending.md); engine via `git show 68eab00:<path>` in D:/Projects/mailclient/opensource/email-calendar-sync-engine.
- AR1-AR7 apply.

---

### Task 1: submit arm, outbox family, Sending surface, gated then_close

**Files:**
- Modify: `crates/astra-app/src/{protocol/intent.rs (+ a new intent_outbox.rs sibling), protocol.rs, dispatch.rs, drafts.rs (the gated forget)}` + tests; Create: `crates/astra-app/src/{outbox.rs, outbox_tests.rs}`; Modify `crates/astra-viewmodel/src/` (+ sending snapshot + tests)
- Port from: MC `mailcal-app/src/{outbox_ops.rs, mail_ops/send.rs, protocol/intent_outbox.rs}`; the engine's submit/outbox op surface at 68eab00.

**Interfaces:**
- Produces: `Intent::SubmitRichMail { from: AccountId, to, cc, bcc, subject, document: ComposerDocument, blobs: Vec<ComposerBlob> }`; `OutboxIntent::{Show, Cancel{queued: QueuedRef}, SendNow{queued: QueuedRef}, ConfirmSent, ConfirmNotSent}` + `Intent::Outbox(OutboxIntent)`; `QueuedRef { account, op: PendingOpId }` in reference.rs; `Surface::Sending` (event `sending`, non-debounced) + `SendingSnapshot { rows: Vec<QueuedRow { queued: QueuedKey, state, subject, arrived_at }> }` in viewmodel; submit renders the document via astra-composer and calls engine submit_mail, then drain; the outbox arms map to engine ops (READ cancel_pending_op / retry_pending_op_now / pending-op confirmation signatures at 68eab00 FIRST); drafts.rs: the then_close forget moves under the save gate (close_composition's shape), the SP2d race test extended to the then_close path.

- [ ] Step 1: protocol wire tests first (submit + all five outbox variants + QueuedRef).
- [ ] Step 2: viewmodel sending snapshot wire tests.
- [ ] Step 3: outbox_tests first (in-memory engine + fakes with an SMTP leg; extend the fakes): submit queues; drain delivers; SendNow retries now; Cancel withdraws; unconfirmed surfaces prompt rows; ConfirmSent/ConfirmNotSent resolve; Sending publishes per transition; the then_close race extended.
- [ ] Step 4: Implement; all green; commit `feat: add submit, the outbox family, and the sending surface`.

### Task 2: SMTP provider binding at connect

**Files:**
- Modify: `crates/astra-account/src/{imap.rs or a new smtp.rs, config.rs (the [smtp] leg into the engine's submission shape)}` + tests
- Port from: MC `mailcal-account` provider registration (how MC binds the SMTP submission provider beside IMAP); engine provider-imap's submission types at 68eab00.

**Interfaces:**
- Produces: `connect_mail_providers` (or its caller) registering the account's submission provider from `[smtp]` (host/port/security + credentials) so the engine drain can deliver; the gated live harness account's SMTP binding works against 127.0.0.1:2025 with the certificate exception flowing the same way.

- [ ] Step 1: Tests first (fakes): the registered provider set includes the submission binding for an account with [smtp]; absent [smtp] leaves mail-only (recorded shape).
- [ ] Step 2: Implement + unit green; commit `feat: bind the smtp submission provider at connect`.

### Task 3: host command and IPC wire

**Files:**
- Modify: `client/desktop/src/commands/**` (+ sibling tests), `client/web/src/services/{ipc.ts, ipc.test.ts}`, `client/web/src/events/surfaceEvents.ts` (+ tests)

**Interfaces:**
- Produces: `sending() -> SendingSnapshot` pull; ipc.ts: SubmitRichMail + OutboxIntent constructors + SendingSnapshot types, literals mirrored; surfaceEvents gains `sending`.

- [ ] Step 1: Tests first (commands pull + wire both sides + events).
- [ ] Step 2: Implement; commit `feat: expose sending over ipc`.

### Task 4: the composer Send button and the outbox panel

**Files:**
- Modify: `client/web/src/features/composer/**` (Send button: dispatch SubmitRichMail with the CURRENT document, close on success, typed-error keep-open; Close switches to Save{then_close: true}); Create: `client/web/src/features/outbox/{OutboxPanel.tsx + tests}`; toast wiring
- [ ] Step 1: Tests first per D5 (send success/failure, close flush payload switch, outbox rows + actions exact payloads, unconfirmed prompts, toast on the surface signal).
- [ ] Step 2: Implement; commit `feat: add the send button and the outbox panel`.

### Task 5: live delivery proof, contract doc, and capabilities

**Files:**
- Modify: `crates/astra-app/src/tests_live_harness/**` (+ send test), `docs/sending.md` (+ zh mirror), `docs/capabilities.md` + zh mirror, `docs/changelog/unreleased/outbox-sending.md`

- [ ] Step 1: Gated send test: connect astra1 (IMAP + SMTP bound, cert accepted) -> SubmitRichMail to astra2 -> drain -> outbox empty/delivered -> sync astra2 -> its INBOX holds the mail (subject pinned).
- [ ] Step 2: Run live: harness up, all gated green (dial/list/detection/reading/drafts/send), slice-check PASS; record outputs.
- [ ] Step 3: Port docs/sending.md in substance from MC (never-send-twice, durable queue, unconfirmed-delivery semantics, cancel and send-now) + zh mirror; capabilities rows (sending partial, outbox partial) + zh mirror + fragment.
- [ ] Step 4: Full battery; commit `feat: prove live delivery and record the sending contract`.

### Task 6: close-out

- [ ] Step 1: Full battery green; branch `sp2e-outbox-sending` pushed; PR opened linking the spec (naming the SP2d prerequisite discharged); acceptance evidence; ledger cleanup.

## Self-review notes

- Spec coverage: D1 (T1 submit), D2 (T1 family+surface), D3 (T1 gated forget + T4 close switch), D4 (T2), D5 (T4), live proof + contract doc (T5).
- Interface chain: submit/outbox intents + snapshot (T1) -> SMTP binding (T2) -> ipc (T3) -> UI (T4) -> live (T5).
- Risks carried: the engine's pending-op confirmation semantics (ConfirmSent/NotSent map to engine ops; READ them at 68eab00 before adapting; if the engine lacks a direct confirm op, port MC's bindings-layer mechanism and record it); the fakes' SMTP leg must honestly model retryable refusal for the unconfirmed path.
