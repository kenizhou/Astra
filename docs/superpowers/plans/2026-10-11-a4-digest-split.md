# A4 Digest, Analytics, Split Inbox Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the digest and report documents with their generation services and panel, the split inbox derived from stored categorisation with the list toggle, the eval fixtures, and the live gated proof.

**Architecture:** Both documents generate host-side over the A1 seams with stored-categorisation rollups; the split map serves from ai_results without model calls; the CLI reads.

**Tech Stack:** Existing workspace; the provided endpoint for the live gate.

**Spec:** `docs/superpowers/specs/2026-10-11-a4-digest-split-design.md`

## Global Constraints

- House rules as every slice: SPDX, 500-line cap, deny-everything lints, gate before commit (direct exit), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- The connection env (never committed): as prior slices; live agents request the relay via the controller.
- Security floor: mail content untrusted-wrapped; the split map is pure derivation (no model calls); the human-sends rule unchanged.

---

### Task 1: the ai_documents store
**Files:** `crates/astra-ai/src/migrations/0010_ai_documents.sql` + store APIs + tests.
**Interfaces:** insert-or-replace keyed (account, kind, period_start, period_end); latest-per-(account, kind) read; list for the CLI.
- [ ] Tests first: round trip; the same-period replace; the kind separation.
- [ ] Implement; commit `feat: add the ai documents store`.

### Task 2: the services
**Files:** `crates/astra-app/src/ai_docs/{mod.rs, digest.rs, digest_tests.rs, report.rs, report_tests.rs, split.rs, split_tests.rs}`.
**Interfaces:** `generate_digest(account, days, messages, stored_categories, correlation, seam) -> String` (the rollup; head-five per category; untrusted-wrapped; DIGEST_PROMPT_VERSION; DEFAULT); `generate_report(account, days, messages, stored_categories, sent_count, correlation, seam) -> String` (stats then one structured call; REPORT_PROMPT_VERSION); `split_map(stored_categories) -> Vec<(key, SplitSide)>` + the pinned mapping const SPLIT_MAPPING_VERSION.
- [ ] Tests first per the spec's unit list.
- [ ] Implement; commit `feat: add the digest, report, and split services`.

### Task 3: eval fixtures
**Files:** `evals/` fixtures for the three constants; runner registration.
- [ ] Tests first: fixtures pin; a scratch bump fails.
- [ ] Implement; commit `feat: pin the a4 evals`.

### Task 4: host commands + ipc
**Files:** `client/desktop/src/commands/ai.rs` grows (+ tests); `client/web/src/services/ai.ts` (+ tests).
**Interfaces:** `digest_generate(account, days?)`, `report_generate(account, days?)`, `latest_document(account, kind)`, `split_map(account)`; wire-pinned.
- [ ] Tests first; commit `feat: expose the a4 commands over ipc`.

### Task 5: the UIs
**Files:** `client/web/src/features/ai-panel/{AiPanel.tsx + tests}` (digest/report tabs); the message list split toggle (+ tests).
- [ ] Tests first: the panel lifecycle; the toggle filters + badges + the uncategorised rule.
- [ ] Implement; commit `feat: add the ai panel and the split inbox toggle`.

### Task 6: CLI + live proof + docs
**Files:** `cli/src/verbs/ai.rs` grows `digest`/`report` reads + the refused `digest generate`/`report generate` spellings + tests; `client/desktop/src/ai/live_docs_tests.rs` (gated); `docs/ai-digest.md` + zh; capabilities + zh (digest/report/split partial; meeting-briefs row with the named dependency); fragments.
- [ ] CLI tests first; the gated live suite; docs; the full battery.
- [ ] Commit `feat: prove a4 live and record the digest contract`.

### Task 7: close-out
- [ ] Full battery; branch `a4-digest-split` pushed; PR (squash expected); acceptance evidence; ledger cleanup.

## Self-review notes

- Spec coverage: D1 (T1/T2/T4/T5/T6), D2 (T1/T2/T4/T5/T6), D3 (T2 split/T4/T5), D4 (T6 docs row), evals (T3), live + docs (T6).
- Interface chain: store (T1) -> services (T2) -> evals (T3) -> ipc (T4) -> UI (T5) -> CLI + live + docs (T6).
- Risks: the window read's cost (31 days worst case; the cap pinned); the split map's pull shape (additive; alongside snapshot); the panel placement choice recorded.
