# SP2h Calendar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver calendar end to end: the Calendar surface with the snapshot, the intent family over the engine's calendar reads/writes, recurrence handling, the invitation flow with the iMIP fallback, the grid/agenda UIs, the meeting briefs, the live CalDAV proof, and the contract docs.

**Architecture:** astra-app owns the CalendarIntent dispatch, the snapshot rebuild over the engine's calendar reads, the invitation machine, and the brief service; the engine owns calendars, occurrence expansion, event persistence, and reply submission.
**Tech Stack:** Existing workspace; the engine's calendar surface at 68eab00 (READ FIRST); Stalwart's CalDAV (verify) for live proof.
**Spec:** `docs/superpowers/specs/2026-10-11-sp2h-calendar-design.md`

## Global Constraints

- House rules as every slice: SPDX, 500-line cap, deny-everything lints, gate before commit (direct exit), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- Standing: AUTO-CONTINUE the moment the PR merges; SQUASH AND MERGE; the LLM relay on request (the briefs live test needs it); the 10-minute author window.
- The engine owns protocol and recurrence; Astra renders and dispatches.

---

### Task 1: the engine survey + protocol/snapshot types
**Files:** the survey note in the report (no code); `crates/astra-app/src/protocol/intent_calendar.rs` + tests; `crates/astra-viewmodel/src/calendar.rs` + tests (NEW; independent of the viewmodel rows).
**Interfaces:** the CalendarIntent family (serde snake_case pinned); `CalendarSnapshot { calendars, occurrences, write_states, now_data }` shapes (the view types minimal: calendar rows, occurrence rows with recurrence markers, write states).
- [ ] Step 0: THE SURVEY (read at 68eab00: calendars, occurrences_in, events_by_keys, the submit/edit paths, invitation reply, can_write surfacing; Stalwart's CalDAV availability in the harness image). Record findings in the report; they bind Tasks 2-3.
- [ ] Tests first; implement; commit `feat: add the calendar protocol and snapshot types`.

### Task 2: the dispatch + snapshot over the engine
**Files:** `crates/astra-app/src/calendar/{mod.rs, snapshot.rs, edit.rs, invite.rs}` + tests (the dispatch arms; the snapshot rebuild; CRUD mapping; the invitation machine; write gating; drag math).
- [ ] Tests first per D2-D4/D6 unit list; implement; commit `feat: dispatch calendar intents over the engine`.

### Task 3: briefs + host/ipc
**Files:** `crates/astra-app/src/calendar/brief.rs` + tests; `client/desktop/src/commands/` (+ the calendar pull command or the surface pull as the survey dictates) + tests; `client/web/src/services/**` (+ tests; the ipc.ts SPLIT lands first; it sits at 500).
- [ ] Tests first; implement; commit `feat: add calendar briefs and the host commands`.

### Task 4: the calendar UI
**Files:** `client/web/src/features/calendar/**` (the view switch entry, the agenda, the three grids, the event dialogs, the invitation card, the briefs panel) + the reading pane invitation card mount + tests.
- [ ] Tests first per D1/D4/D5 UI list; implement; commit `feat: render the calendar views and invitations`.

### Task 5: live proof + docs
**Files:** the gated live calendar tests (the CalDAV round trip per the survey; the RSVP; the brief over seeded events with the relayed LLM); `docs/calendar.md` + `docs/invitations.md` + zh; capabilities + zh (calendar, invitations, meeting briefs rows move); the fragment; the full battery.
- [ ] Commit `feat: prove the calendar live and record the contracts`.

### Task 6: close-out
- [ ] Full battery; branch `sp2h-calendar` pushed; PR (squash expected); acceptance evidence; ledger cleanup.

## Self-review notes

- Spec coverage: D1 (T1 types/T4 UI), D2 (T1/T2), D3 (T2), D4 (T2/T4), D5 (T3/T4), D6 (T2/T4), live + docs (T5).
- Interface chain: the survey (T1) binds everything; types (T1) -> dispatch/snapshot (T2) -> briefs/ipc (T3) -> UI (T4) -> live + docs (T5).
- Risks: the engine's calendar surface breadth (the survey decides what ports now vs defers, never silently); Stalwart CalDAV (verify; the fallback is the recorded gap); the grid complexity (split files hard); the ipc.ts split MUST land in T3 before adding wire.
