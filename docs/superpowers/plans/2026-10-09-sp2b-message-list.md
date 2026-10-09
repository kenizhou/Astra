# SP2b Message List Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Grow the MailboxList snapshot with message rows (flat and threaded), add MarkRead and SetFlagged over RowRef, render the list pane with virtualization and actions, and prove it live against seeded harness messages.

**Architecture:** Additive snapshot growth in astra-viewmodel; reads through engine mail_window / mail_on_threads via the existing row cache and generation guard; writes through engine edit_mail with discard-and-resync; two new serde Intents; the SPA list pane goes real with @tanstack/react-virtual.

**Tech Stack:** Existing Rust workspace; @tanstack/react-virtual (kylins' version) added to client/web.

**Spec:** `docs/superpowers/specs/2026-10-09-sp2b-message-list-design.md`

## Global Constraints

- Same house rules as SP1/SP2a: SPDX, 500-line cap, deny-everything lints, gate before commit, conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only (existing fixtures must pass unchanged).
- Port source root MC = D:/Projects/mailclient/opensource/mail-calendar; kylins root KY = D:/Projects/mailclient/kylins.
- Adaptation rules AR1-AR7 from the prior plans apply.

---

### Task 1: viewmodel rows and snapshot growth

**Files:**
- Modify: `crates/astra-viewmodel/src/{view.rs, view_tests.rs, folders.rs}`; new `crates/astra-viewmodel/src/rows.rs` (+ tests) if the cap demands
- Port from: MC `mailcal-viewmodel/src/view.rs` message row types.

**Interfaces:**
- Produces: `MessageRowView` (row key: message or thread id, sender display name + address, subject, sent date, unread, flagged, has_attachments, thread_count for threaded mode, account id) with serde (skip_serializing_if on Option/defaulted fields); `MailboxListSnapshot` gains `rows: Vec<MessageRowView>`, `selection: Option<RowKey>`, `visible_limit`, `view_mode`, all `#[serde(default)]` so old fixtures parse.

- [ ] Step 1: Tests first: old fixture (no new fields) round trips; new fixture with rows pins the exact wire; view mode serialises snake_case.
- [ ] Step 2: Port the row projection types (MC's row shapes trimmed to these fields); date as MC carries it (check MC's row date type and mirror).
- [ ] Step 3: Tests/clippy/fmt/gate green; commit `feat: grow the mailbox snapshot with message rows`.

### Task 2: RowRef, new intents, list reads

**Files:**
- Modify: `crates/astra-app/src/{reference.rs, protocol/intent.rs, protocol_tests.rs, dispatch.rs, snapshot.rs, row_cache.rs, accounts.rs if needed}` + tests; `client/web/src/services/ipc.ts` (+ tests)
- Port from: MC `mailcal-app/src/{reference.rs (RowRef/MessageRef/ThreadRef), dispatch.rs (MarkRead/SetFlagged arms), snapshot.rs (windowing/list projection)}`.

**Interfaces:**
- Produces: `RowRef::{Message, Thread}` with from_parts; `MessageRef`, `ThreadRef`; `Intent::MarkRead { row: RowRef, read: bool }`, `Intent::SetFlagged { row: RowRef, flagged: bool }`; snapshot rows filled per selected folder (flat: engine mail_window; threaded: engine mail_on_threads); ShowMore grows; SetViewMode re-projects and resets; selection field driven by folder selection initially.
- IPC: ipc.ts Intent union grows the two variants (wire tests pin `{"mark_read":{"row":{"message":{...}},"read":true}}` style literals from the Rust tests).

- [ ] Step 1: protocol_tests first: both variants round trip exact wire; RowRef from_parts rejects malformed.
- [ ] Step 2: dispatch_tests first (in-memory engine + fakes): SelectFolder rebuilds rows; ShowMore grows; view mode switch re-projects and resets; MarkRead/SetFlagged write via edit_mail fakes and the snapshot republishes with flipped row state.
- [ ] Step 3: Implement (port MC arms/projection; engine edit_mail is the write seam; check the engine's edit_mail signature at the pinned rev and adapt MC's calls).
- [ ] Step 4: ipc.ts + wire tests; all green; commit `feat: add message list reads and read/flag intents`.

### Task 3: frontend list pane

**Files:**
- Modify: `client/web/src/components/layout/{AppShell.tsx, MessageList.tsx}` (replace the placeholder), new `client/web/src/components/message-list/{MessageRow.tsx, Monogram.tsx, ListToolbar.tsx}`; `client/web/src/stores/{folderStore.ts or a new listStore.ts}`; `client/web/package.json` (+ @tanstack/react-virtual matching KY's version)
- Port from: KY `kylins.client.frontend` MessageList/virtualization patterns; MC row presentation semantics.

- [ ] Step 1: Tests first: MessageList renders rows from a fixture (virtualizer windowed; assert first N render and scrolling loads more via ShowMore button or scroll hook per KY pattern); view mode toggle dispatches set_view_mode; read/flag buttons dispatch mark_read/set_flagged with the exact RowRef payload; unread rows bold, flag glyph on flagged.
- [ ] Step 2: Implement; folder selection drives the list (from folderStore selected folder); monogram from sender address.
- [ ] Step 3: vitest + build + gate green; commit `feat: render the message list pane with actions`.

### Task 4: harness seeding and live list test

**Files:**
- Modify: `docker/harness/**` (seed three messages to astra1 INBOX via the harness SMTP), `scripts/dev/harness.sh` (idempotent seeding), `crates/astra-app/src/tests_live_harness.rs` (+ list test)
- Modify: `docs/capabilities.md` + zh mirror; `docs/changelog/unreleased/message-list.md`

- [ ] Step 1: Extend the seed script to submit three distinct messages to astra1@example.test through 127.0.0.1:2025 (self-signed TLS; the submission client accepts the harness cert the same way the gated IMAP test does); idempotent on warm volumes.
- [ ] Step 2: Gated list test: harness up -> connect + sync -> assert three rows in INBOX (subjects pinned) -> MarkRead one -> RefreshMail -> assert unread count drops and the row shows read.
- [ ] Step 3: Run live: harness up, gated tests green, slice-check still PASS; record outputs.
- [ ] Step 4: capabilities rows (message list -> partial; threading -> partial for grouped view) + zh mirror + fragment (Platforms: desktop, Bump: minor).
- [ ] Step 5: Full battery; commit `feat: prove the message list live and record the capability`.

### Task 5: close-out

- [ ] Step 1: Full battery green; branch `sp2b-message-list` pushed; PR opened linking the spec; acceptance criteria evidence.
- [ ] Step 2: Ledger cleanup per the SDD skill.

## Self-review notes

- Spec coverage: D1 (Task 1), D2/D3 (Task 2), D4 (Task 2 write path), D5 (Task 3), live proof (Task 4), docs (Task 4.4).
- Interface chain: MessageRowView/RowKey (T1) -> RowRef/intents/reads (T2) -> ipc wire (T2) -> list pane (T3) -> live proof (T4).
- Risk carried: engine edit_mail/mail_on_threads signatures at the pinned rev must be read before porting MC calls (Task 2 step 3 names it).
