# SP2f Reply and Forward Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver reply, reply all, and forward end to end: the Compose family computing prefilled compositions server-side with quoted content, the ComposeRequest surface hydrating the composer, the two submit intents threading through the durable outbox, live reply/forward delivery proof.

**Architecture:** astra-app gains the Compose family arm (header reads via the engine, quote building via astra-composer blocks, ComposeRequest snapshot), the two submit intents beside SubmitRichMail sharing the outbox path; the SPA reading pane dispatches the family and the composer hydrates from the request surface.

**Tech Stack:** Existing workspace; engine message reads at the pinned rev; MC's reply/forward contracts (mailcal-app reply paths, ComposeRequest surface) as the shape source.

**Spec:** `docs/superpowers/specs/2026-10-09-sp2f-reply-forward-design.md`

## Global Constraints

- House rules as prior slices: SPDX, 500-line cap, deny-everything lints, gate before commit (direct exit, never piped), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- Port roots: MC = D:/Projects/mailclient/opensource/mail-calendar (mailcal-app reply/forward + ComposeRequest, mailcal-composer quote blocks); engine via `git show 68eab00:<path>` in D:/Projects/mailclient/opensource/email-calendar-sync-engine.
- AR1-AR7 apply.

---

### Task 1: Compose family, quote building, ComposeRequest surface

**Files:**
- Modify: `crates/astra-app/src/{protocol/intent.rs, protocol.rs, dispatch.rs}` + tests; Create: `crates/astra-app/src/{compose_request.rs, compose_request_tests.rs}`; Modify `crates/astra-viewmodel/src/` (+ ComposeRequest snapshot + tests)
- Port from: MC `mailcal-app` reply paths + ComposeRequest surface; the engine's message header reads at 68eab00.

**Interfaces:**
- Produces: `ComposeIntent::{Reply{message: MessageRef}, ReplyAll{message}, Forward{message}}` + `Intent::Compose(ComposeIntent)` (#[non_exhaustive] family like Folders); `Surface::ComposeRequest` (event `compose_request`, non-debounced) + `ComposeRequestSnapshot` in viewmodel (kind: Reply | ReplyAll | Forward, to, cc, subject, in_reply_to: Option<String>, document: ComposerDocument with the quote block, from account); the arm reads the original's headers via the engine (READ the message read signature at 68eab00 first), computes recipients per MC rules (reply: sender; reply-all: sender + to + cc minus self; forward: empty), prefixes subjects per MC (Re:/Fwd: with existing-prefix awareness), builds the Quote block with the attribution line, and publishes.

- [ ] Step 1: protocol wire tests first (three family variants + the compose_request surface word).
- [ ] Step 2: viewmodel snapshot wire tests (full reply shape incl. in_reply_to and the quote document; forward without in_reply_to).
- [ ] Step 3: compose_request_tests first (fakes with full headers): recipient rules per kind (self excluded), subject prefixes (no double Re:), attribution line format, plain-text quote from the reading content, html fallback to sanitised text, forward quote, publish per dispatch.
- [ ] Step 4: Implement; all green; commit `feat: add the compose request family with quoted prefill`.

### Task 2: reply and forward submit intents

**Files:**
- Modify: `crates/astra-app/src/{protocol/intent.rs, outbox.rs (the submit seam), dispatch.rs}` + tests
- Port from: MC's SubmitRichReply/SubmitRichForward arms; the engine's draft builder threading (in_reply_to handling) at 68eab00.

**Interfaces:**
- Produces: `Intent::SubmitRichReply { from, to, cc, in_reply_to: MessageIdHeader (newtype from the engine's re-exports), subject, document, blobs }` and `Intent::SubmitRichForward { from, to, subject, document, blobs }`; both share submit_draft's outbox path with the reply's draft carrying in_reply_to (engine Draft builder; READ at 68eab00; if the builder takes the header, pass it; record the shape).

- [ ] Step 1: protocol wire tests first (both intents incl. in_reply_to round trip).
- [ ] Step 2: submit tests first (fakes record the draft): reply's draft carries in_reply_to; forward does not; both queue and drain like SubmitRichMail; never-send-twice unchanged.
- [ ] Step 3: Implement; all green; commit `feat: add reply and forward submit intents`.

### Task 3: host command and IPC wire

**Files:**
- Modify: `client/desktop/src/commands/**` (+ sibling tests), `client/web/src/services/**` (+ tests), `client/web/src/events/surfaceEvents.ts` (+ tests)

**Interfaces:**
- Produces: `compose_request() -> Option<ComposeRequestSnapshot>` pull (the None-clears semantics like reading); ipc.ts: the Compose family + both submit intents + the snapshot types, literals mirrored; surfaceEvents gains `compose_request`.

- [ ] Step 1: Tests first (commands pull/clear; wire both sides; events admit + pull).
- [ ] Step 2: Implement; commit `feat: expose compose requests over ipc`.

### Task 4: reading pane actions and composer hydration

**Files:**
- Modify: `client/web/src/components/reading-pane/**` (Reply / Reply all / Forward buttons dispatching exact payloads), `client/web/src/features/composer/**` (hydrate from the request surface: fields, subject prefix display, quote document, in_reply_to riding the draft save; Send dispatches SubmitRichReply/Forward per the open request kind), `client/web/src/stores/**` (+ composeRequestStore)

- [ ] Step 1: Tests first: buttons dispatch exact Compose intents per the open message; the composer hydrates (fields + quote visible in the editor as a quote block per the trimmed editor's rendering of Quote blocks; retention already round trips it; the editor shows it as retained content); Send dispatches the right submit intent with in_reply_to for replies; closing without sending leaves nothing stale (the request surface cleared on composer close or next request).
- [ ] Step 2: Implement; commit `feat: wire reply and forward into the reading pane and composer`.

### Task 5: live proof and docs

**Files:**
- Modify: `crates/astra-app/src/tests_live_harness/**` (+ reply/forward tests), `docs/sending.md` (+ zh mirror, reply/forward section), `docs/capabilities.md` + zh mirror, `docs/changelog/unreleased/reply-forward.md`

- [ ] Step 1: Gated tests: astra2 replies to astra1's seeded note -> delivered to astra1 with In-Reply-To matching the original's Message-ID (assert via the engine's thread read or a header read); forward delivers with the Fwd: subject; both queues empty; cleanup restores the contracts (astra1 INBOX three notes at rest; the delivered reply must be cleaned or the list test's whole-equality breaks; pick the honest cleanup route and pin).
- [ ] Step 2: Run live: harness up, all gated green (7 suites now), slice-check PASS; record outputs.
- [ ] Step 3: sending.md reply/forward section (recipient rules, quoting, threading semantics, the attachments deferral if it applied) + zh mirror; capabilities rows (reply partial, forward partial, threading partial for reply linkage) + zh mirror + fragment.
- [ ] Step 4: Full battery; commit `feat: prove live reply and forward and record the contract`.

### Task 6: close-out

- [ ] Step 1: Full battery green; branch `sp2f-reply-forward` pushed; PR opened linking the spec; acceptance evidence; ledger cleanup.

## Self-review notes

- Spec coverage: D1 (T2), D2 (T1 quote building), D3 (T1 family + surface), D4 (T4 hydration + in_reply_to riding saves), D5 (T1 plain-text quote paths), live proof + docs (T5).
- Interface chain: Compose family + snapshot (T1) -> submit intents (T2) -> ipc (T3) -> UI (T4) -> live (T5).
- Risks carried: the engine's header reads and the draft builder's in_reply_to shape at 68eab00 (T1/T2 name them: READ FIRST); attachments-on-forward may need the staging flow (defer explicitly if so, record in capabilities + sending.md); the list test's three-note contract must survive the reply delivery (cleanup pinned in T5).
