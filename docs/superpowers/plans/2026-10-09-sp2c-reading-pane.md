# SP2c Reading Pane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the reading pane end to end: ReadingSnapshot and the Reading surface, OpenMessage/CloseReading intents, the Rust body pipeline (sanitise, CID data URIs, remote gating), the sandboxed webview renderer with the remote-image toggle, live proof against a rich seeded message.

**Architecture:** A new pushed surface `Reading` over a single `Surfaced<Option<ReadingSnapshot>>`; the body pipeline lives in astra-app (ammonia sanitising mirroring MC, engine part decoding, data-URI inlining, placeholder rewrite); the SPA renders sanitised HTML in a sandboxed srcdoc iframe and owns only the toggle transform.

**Tech Stack:** Existing workspace plus ammonia 4 (workspace table, MC's pin); engine `message_source` reads; iframe sandbox CSP in the SPA.

**Spec:** `docs/superpowers/specs/2026-10-09-sp2c-reading-pane-design.md`

## Global Constraints

- House rules as SP1/SP2a/SP2b: SPDX, 500-line cap, deny-everything lints, gate before commit (never piped), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- Port source roots: MC = D:/Projects/mailclient/opensource/mail-calendar (read MC's reading path: mailcal-app reading.rs, viewmodel reading.rs, and the rendering-security posture); engine pinned rev via `git show 68eab00:<path>` in D:/Projects/mailclient/opensource/email-calendar-sync-engine.
- Adaptation rules AR1-AR7 apply.

---

### Task 1: ReadingSnapshot in the viewmodel

**Files:**
- Create: `crates/astra-viewmodel/src/{reading.rs, reading_tests.rs}`; modify `lib.rs`

**Interfaces:**
- Produces: `ReadingSnapshot` (message: MessageKey { account, key }, subject, from_name, from_address, sent_at, html: Option<String>, plain: Option<String>, attachments: Vec<AttachmentRow>, remote_image_count: u32), `AttachmentRow` (id: u32, file_name, media_type, size: u64); serde per house style (skip_serializing_if on Options and empty Vec, rename_all snake_case where needed).

- [ ] Step 1: Wire tests first: full literal (html + plain + attachments + count 2), minimal literal (plain only, no optional keys on the wire), round trip both.
- [ ] Step 2: Implement; lib re-exports.
- [ ] Step 3: Tests/clippy/fmt/gate green; commit `feat: add the reading snapshot view model`.

### Task 2: intents and the body pipeline in astra-app

**Files:**
- Modify: `crates/astra-app/src/{protocol/intent.rs, protocol.rs, reference.rs (re-export if needed), dispatch.rs}` + tests; Create: `crates/astra-app/src/{reading.rs, reading_tests.rs, sanitise.rs, sanitise_tests.rs}`; modify `Cargo.toml` (ammonia workspace dep) and the workspace table
- Port from: MC `mailcal-app/src/{reading.rs, sanitise or equivalent}` and its tests; engine reads at the pinned rev.

**Interfaces:**
- Produces: `Intent::OpenMessage { message: MessageRef }`, `Intent::CloseReading` (wire pinned); `Surface::Reading` variant (event name `reading`, non-debounced); `Surfaced<Option<ReadingSnapshot>>` in App; the pipeline fn `render_reading(engine, account, message) -> ReadingSnapshot` (sanitise per MC's ammonia config; CID parts to data URIs with a size cap constant; remote src to `data-astra-src` with count); open marks read via the existing write path; RefreshMail republishes or closes on gone.

- [ ] Step 1: protocol wire tests first (both intents, reading event name).
- [ ] Step 2: sanitise tests first: port MC's sanitisation vectors (script strip, style/jscript policy, allowed tags/attributes table, URL schemes); CID cases (inline part -> data URI, oversized skip with alt, missing part leaves cid untouched for plain); remote cases (http/protocol-relative src rewritten and counted, data: and cid: untouched post-inline).
- [ ] Step 3: dispatch tests first (in-memory engine + fakes serving a source with parts): open publishes Reading and marks read (unread flips, survives refresh); close clears; refresh with the message gone closes; refresh with it present republishes fresh content.
- [ ] Step 4: Implement (read the engine's message_source/attachment signatures at 68eab00 before adapting MC's calls).
- [ ] Step 5: All green; commit `feat: add the reading pipeline and open/close intents`.

### Task 3: host command and IPC wire

**Files:**
- Modify: `client/desktop/src/{commands.rs, commands_tests.rs, observer.rs if the surface list is enumerated}`, `client/web/src/services/{ipc.ts, ipc.test.ts}`

**Interfaces:**
- Produces: `reading() -> Option<ReadingSnapshot>` pull command (serde straight through); `surface_changed` payload carries `reading`; ipc.ts ReadingSnapshot/AttachmentRow types + the two intents; wire tests pin literals from the Rust side.

- [ ] Step 1: commands_tests first: pull returns the published snapshot (None after close); no panic paths.
- [ ] Step 2: Implement + register; ipc.ts types + wire tests (mirror the Rust reading_tests literals).
- [ ] Step 3: All green; commit `feat: expose the reading surface over ipc`.

### Task 4: the reading pane in the SPA

**Files:**
- Modify: `client/web/src/components/layout/{AppShell.tsx, ReadingPane.tsx}` (replace placeholder); Create: `client/web/src/components/reading-pane/{MessageHtml.tsx, AttachmentList.tsx, ReadingHeader.tsx}`; `client/web/src/stores/readingStore.ts`; extend `client/web/src/events/surfaceEvents.ts` (reading -> pull) and the message list (open on selection)

- [ ] Step 1: Tests first: opening a row (click) dispatches `open_message` with the exact payload and the pane renders header (subject/from/date) from the snapshot; sandboxed iframe gets the sanitised srcdoc (assert sandbox attrs + csp img-src data:); remote images absent in the doc and `remote_image_count` shown; the toggle flips placeholder to src and the count clears; attachments render metadata rows; plain fallback when no html; empty state when none; close button dispatches `close_reading`; selection highlight follows the open message.
- [ ] Step 2: Implement; single-click in the list opens (selection = open; record).
- [ ] Step 3: vitest + build + gate green; commit `feat: render the reading pane with sandboxed html and remote gating`.

### Task 5: rich harness message and live proof

**Files:**
- Modify: `docker/harness/stalwart/seed.sh` (a rich MIME message into Projects: HTML body + remote image + inline CID png + one PDF-ish attachment; idempotent by subject search), `crates/astra-app/src/tests_live_harness.rs` (+ reading test)
- Modify: `docs/capabilities.md` + zh mirror; `docs/changelog/unreleased/reading-pane.md`

- [ ] Step 1: Extend the seed (hand-built MIME with a tiny valid PNG inline part and a small attachment; subject "Harness rich note"); keep the INBOX three-note contract intact (this message lives in Projects).
- [ ] Step 2: Gated reading test: open the rich message -> sanitised html present, remote_image_count >= 1, a data:image/png URI inlined, one attachment row, unread flips to read surviving RefreshMail.
- [ ] Step 3: Run live: harness up, gated tests green, slice-check PASS, list test still green (three INBOX notes untouched); record outputs.
- [ ] Step 4: capabilities rows (reading -> partial, rendering security -> partial) + zh mirror + fragment (Platforms: desktop, Bump: minor).
- [ ] Step 5: Full battery; commit `feat: prove the reading pipeline live and record the capability`.

### Task 6: close-out

- [ ] Step 1: Full battery green (gate not piped, workspace tests, cargo doc, desktop build, web build + vitest); tree clean.
- [ ] Step 2: Branch `sp2c-reading-pane` pushed; PR opened linking the spec; acceptance evidence; ledger cleanup per the SDD skill.

## Self-review notes

- Spec coverage: D1 (T1), D2/D3 (T2), D4 (T4), D5 (pipeline design T2 + iframe T4), host seam (T3), live proof + docs (T5).
- Interface chain: ReadingSnapshot (T1) -> pipeline + intents (T2) -> ipc wire (T3) -> pane (T4) -> live proof (T5).
- Risk carried: the engine part/attachment decode surface at 68eab00 must be read before porting MC's body decode (T2 step 4 names it); the iframe CSP attribute support in WebView2 needs a runtime check (T4 keeps the sandbox attrs asserted in jsdom and honest in the report).
