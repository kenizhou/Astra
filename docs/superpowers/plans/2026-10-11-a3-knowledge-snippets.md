# A3 Knowledge, Snippets, Autocomplete, Translate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the knowledge store with batch extraction and draft-prompt shaping, the snippet library with AI search and the composer picker, the autocomplete suggestion with the debounced chip, the cached translate action, the host commands and CLI verbs, the eval fixtures, and the live gated proof.

**Architecture:** Extraction/search/autocomplete/translate run host-side over the A1 client seams; astra-app owns the stores and services; the composer and reading pane consume; the CLI reads and manages CRUD.

**Tech Stack:** Existing workspace; the provided DeepSeek endpoint for the live gate.

**Spec:** `docs/superpowers/specs/2026-10-11-a3-knowledge-snippets-design.md`

## Global Constraints

- House rules as every slice: SPDX, 500-line cap, deny-everything lints, gate before commit (direct exit), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- The connection env (never committed): as A1/A2.
- Security floor: mail content untrusted-wrapped in every prompt; knowledge entries are model-derived data on the trusted half; autocomplete never inserts without a gesture; the human-sends rule unchanged.

---

### Task 1: stores and use cases
**Files:** `crates/astra-ai/src/migrations/{0007_knowledge.sql,0008_snippets.sql,0009_text_cache.sql}` + store APIs + tests; `LlmUseCase::{KnowledgeExtract, SnippetSearch, Autocomplete, Translate}` added (ECONOMY, ECONOMY, CHAT, DEFAULT).
**Interfaces:** knowledge entry CRUD (list per account, insert with dedup guard, forget by id); snippets CRUD (list newest-first, create, delete, update touch); the text cache keyed (account, key, kind, lang nullable, body_hash) with get/put.
- [ ] Tests first: round trips per store; the dedup guard; the cache kind/lang keying.
- [ ] Implement; commit `feat: add the knowledge, snippet, and text cache stores`.

### Task 2: the services
**Files:** `crates/astra-app/src/ai_assist/{mod.rs, knowledge.rs, knowledge_tests.rs, snippets.rs, snippets_tests.rs, autocomplete.rs, autocomplete_tests.rs, translate.rs, translate_tests.rs}`.
**Interfaces:** `extract_knowledge(messages, account, correlation, seam) -> Vec<KnowledgeEntry>` (structured; source-tracked; the 30-message cap; the dedup at insert); `knowledge_context(account) -> Vec<String>` (the ten-entry kind-ordered cap for the draft prompt); `search_snippets(query, snippets, correlation, seam) -> Vec<Snippet>` (one ranking call, substring fallback); `autocomplete(before, after, correlation, seam) -> String` (the versioned prompt; CHAT); `translate(text, lang, correlation, seam, cache)` (cached; DEFAULT). Version consts: KNOWLEDGE_EXTRACT, SNIPPET_SEARCH, AUTOCOMPLETE, TRANSLATE.
- [ ] Tests first per the spec's unit list (shapes, caps, fallbacks, cache no-call).
- [ ] Implement; commit `feat: add the knowledge, snippet, autocomplete, and translate services`.

### Task 3: draft prompt shaping + eval fixtures
**Files:** `crates/astra-app/src/ai_replies/draft.rs` (the knowledge context slot; DRAFT_REPLY_PROMPT_VERSION bumped to "2"); `evals/` fixtures for the four new constants + the draft bump; eval runner registration.
- [ ] Tests first: the knowledge entries ride the trusted half as data (pinned); the draft fixture's version pin moves to "2"; the four new fixtures pin their constants.
- [ ] Implement; commit `feat: shape drafts with knowledge and pin the a3 evals`.

### Task 4: host commands and ipc
**Files:** `client/desktop/src/commands/ai.rs` grows (+ sibling tests); `client/web/src/services/{ai.ts, ai.test.ts}`; events where needed (knowledge extraction progress? keep simple: the command returns when done; the UI shows a spinner).
**Interfaces:** knowledge_list/knowledge_extract/knowledge_forget, snippet_list/snippet_search, compose_autocomplete, translate; wire-pinned both sides.
- [ ] Tests first: command round trips via the fake seams; wire literals.
- [ ] Implement; commit `feat: expose the a3 commands over ipc`.

### Task 5: the UIs
**Files:** `client/web/src/features/composer/{SnippetPicker.tsx, AutocompleteChip.tsx}` + tests; `client/web/src/components/reading-pane/TranslateBlock.tsx` + tests; wiring.
- [ ] Tests first: the picker (list/search/insert at cursor), the chip (debounce/dismiss/gesture-only insert), the translate block (lang pick/cached/fresh/error).
- [ ] Implement; commit `feat: add the snippet picker, autocomplete chip, and translate block`.

### Task 6: CLI verbs + live proof + docs
**Files:** `cli/src/verbs/{knowledge.rs, snippets.rs}` + tests (reads + CRUD with --preview; extraction/search/autocomplete/translate never from the CLI); `client/desktop/src/ai/live_assist_tests.rs` (gated); `docs/ai-assist.md` + zh; capabilities + zh; fragments.
- [ ] CLI tests first; the gated live suite per the spec's live list; docs; the full battery.
- [ ] Commit `feat: prove a3 live and record the assist contract`.

### Task 7: close-out
- [ ] Full battery; branch `a3-knowledge-snippets` pushed; PR (squash expected) linking the spec; acceptance evidence; ledger cleanup.

## Self-review notes

- Spec coverage: D1 (T1/T2/T3), D2 (T1/T2/T5/T6), D3 (T2/T4/T5), D4 (T1/T2/T4/T5), D5 (T4/T6), evals (T3), live + docs (T6).
- Interface chain: stores (T1) -> services (T2) -> draft shaping + evals (T3) -> ipc (T4) -> UI (T5) -> CLI + live + docs (T6).
- Risks: the draft prompt bump is a real prompt change (the eval rerun must land in the same change, per the AI rule); the autocomplete debounce in jsdom (fake timers); the TipTap cursor insertion seam (insert at the current selection; verify the editor API at the pinned version).
