# A2 Reply Drafting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]` syntax for tracking.

**Goal:** Deliver the draft pool with the surfacing gate and smart-reply chips, the cached summarise action, the per-account style profile, the host commands and CLI verbs, and the live gated proof.

**Architecture:** Generation/summarise/infer run host-side over the A1 client seams; astra-app owns the pool, cache, profiles, and the OpenMessage arming; the reading pane consumes chips and summaries; the composer accepts a chip as a prefilled reply.

**Tech Stack:** Existing workspace; the provided DeepSeek endpoint for the live gate.

**Spec:** `docs/superpowers/specs/2026-10-10-a2-reply-drafting-design.md`

## Global Constraints

- House rules as every slice: SPDX, 500-line cap, deny-everything lints, gate before commit (direct exit), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- The connection env (never committed): as A1.
- Security floor: mail content untrusted-wrapped in every prompt; the style note is data; chips never auto-send anything; the human-sends rule unchanged.

---

### Task 1: store growth and the draft service
**Files:** `crates/astra-ai/src/migrations/{0004_draft_pool.sql,0005_summarise_cache.sql}` + store APIs + tests; `crates/astra-app/src/ai_replies/{mod.rs, draft.rs, draft_tests.rs}`; `LlmUseCase::StyleInfer` added (ECONOMY).
**Interfaces:** pool row CRUD + query-by-message; cache get/put keyed (message, body_hash); `generate_reply(context..., correlation, AiComplete seam) -> DraftPoolEntry` (structured `{reply, confidence}` via repair; `DRAFT_REPLY_PROMPT_VERSION`); the context collector (original + thread history capped + style note as data); failure -> skipped row.
- [ ] Tests first: pool round trips; cache hit/miss/hash-invalidate; generation shape + repair + failure; the collector caps and wraps.
- [ ] Implement; commit `feat: add the draft pool and reply generation`.

### Task 2: the gate, chips data, summarise, style
**Files:** `crates/astra-app/src/ai_replies/{gate.rs, gate_tests.rs, summarise.rs, summarise_tests.rs, style.rs, style_tests.rs}`.
**Interfaces:** `chips_for(message, pool, judge seam) -> Vec<Chip>` (confidence const + JudgeDraftSurface Yes; top two trimmed; the dismissal marker consulted); `summarise(message, seam, correlation)` with the cache; `style_profile(account, seam, correlation)` (twenty-message cap, no-Sent no-profile, the built_at daily cap, `ai style rebuild` forcing).
- [ ] Tests first per the brief's unit list.
- [ ] Implement; commit `feat: add the chips gate, cached summarise, and style profiles`.

### Task 3: OpenMessage arming and host commands
**Files:** `crates/astra-app/src/{reading.rs (the arming hook), ai_replies/drive.rs}` + tests; `client/desktop/src/ai/seams.rs` grows the arming; `client/desktop/src/commands/ai.rs` (+ tests); `client/web/src/services/{ai.ts, ai.test.ts}`; surfaceEvents or a dedicated event.
**Interfaces:** OpenMessage arms the generation (spawned, one-per-message dedup via pool presence, the reading correlation riding); host commands `draft_chips`, `summarise`, `style_profile`; the `draft_pool` event on row landing; ipc.ts types + wire pins.
- [ ] Tests first: the arming (once per message; failure skips); commands round trip; wire both sides.
- [ ] Implement; commit `feat: arm reply drafting on open and expose the ai commands`.

### Task 4: the reading pane chips and summarise UI
**Files:** `client/web/src/components/reading-pane/{SmartReplies.tsx, SummaryBlock.tsx}` + tests; ReadingPane wiring; the composer accept path (dispatch the SP2f Compose::Reply then hydrate with the draft document; the honest seam: a new host command `compose_from_draft` or riding the existing ComposeRequest with a draft override; pick and record).
- [ ] Tests first per D3/D4 (render, accept dispatch, dismiss, summarise states, the collapsible block).
- [ ] Implement; commit `feat: add smart reply chips and the summarise block`.

### Task 5: CLI verbs
**Files:** `cli/src/verbs/ai.rs` grows `drafts`, `summary`, `style rebuild` + tests; docs/cli.md rows (+ zh).
- [ ] Tests first: reads from the store; rebuild's preview; audit rows.
- [ ] Implement; commit `feat: add the ai replies verbs to the cli`.

### Task 6: live proof and docs
**Files:** `client/desktop/src/ai/live_replies_tests.rs` (gated); `docs/ai-replies.md` (+ zh); capabilities + zh; fragments.
- [ ] Gated suite per D3's live list (draft, gate, summarise cached-second-call, style from Sent, chips end-to-end); the offline battery; docs.
- [ ] Commit `feat: prove a2 live and record the replies contract`.

### Task 7: close-out
- [ ] Full battery; branch `a2-reply-drafting` pushed; PR (squash expected) linking the spec; acceptance evidence; ledger cleanup.

## Self-review notes

- Spec coverage: D1 (T1), D2/D3 data (T2), D4 (T2), D5 (T2), D6 (T3/T5), UI (T4), live + docs (T6).
- Interface chain: pool/cache/profiles (T1) -> gate/summarise/style (T2) -> arming + commands (T3) -> UI (T4) -> CLI (T5) -> live (T6).
- Risks: the chip-accept seam into the SP2f reply flow (T4 decides and records); the thread-history reads at the pinned engine rev (T1 reads first); the style daily cap's clock (built_at comparison; no background timer; demand-driven only, record).
