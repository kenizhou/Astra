# SP2i Contacts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver contacts end to end: the People surface with the snapshot, the intent family over the engine's people/contact reads and writes, the join disclosure, the editor's card-naming discipline, composer autosuggest with pills, avatars, the DAV account extension dialing both legs at boot, the live CardDAV proof, and the contract docs.

**Architecture:** astra-app owns the PeopleIntent dispatch, the snapshot rebuild over the engine's people query, and the write mapping; astra-account grows the DAV leg; the engine owns the join, the ordering, the matching, and the photo cache.
**Tech Stack:** Existing workspace; the engine's contact surface at 68eab00 (READ FIRST); Stalwart's CardDAV (verify) for the live proof.
**Spec:** `docs/superpowers/specs/2026-10-12-sp2i-contacts-design.md`

## Global Constraints

- House rules as every slice: SPDX, 500-line cap, deny-everything lints, gate before commit (direct exit), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- Standing: AUTO-CONTINUE the moment the PR merges; SQUASH AND MERGE; the LLM relay on request (live tests need it); the 10-minute author window.
- The engine owns the join, ordering, and matching; Astra renders and dispatches, and derives the section letter and the monogram once in the rebuild (the engine has no such model at the pinned rev, the spec's recorded deviation). No client-side matching on top.

---

### Task 1: the engine survey + protocol/snapshot types
**Files:** the survey note in the report (no code); `crates/astra-app/src/protocol/intent_people.rs` + tests; `crates/astra-viewmodel/src/people.rs` + tests (NEW).
**Interfaces:** the PeopleIntent family (serde snake_case pinned); the PeopleSnapshot shapes (rows with the disclosure facts and the section letter; the detail with sources, provenance, editable cards; write states; create targets).
- [ ] Step 0: THE SURVEY (read at 68eab00: people_page/PeopleQuery/PeoplePage, person/person_sources, contact_card, contact_targets/editable_cards equivalents, create/patch shapes, recipient_suggestions, contact_photo; Stalwart's CardDAV availability in the harness). Record findings; they bind Tasks 2-3 and 5.
- [ ] Tests first; implement; commit `feat: add the people protocol and snapshot types`.

### Task 2: the dispatch + snapshot over the engine
**Files:** `crates/astra-app/src/people/{mod.rs, snapshot.rs, write.rs, suggest.rs}` + tests (the dispatch arms; the rebuild; the CRUD mapping with the card-naming and patch discipline; the suggestion ranking passthrough).
- [ ] Tests first per D2-D4 unit list; implement; commit `feat: dispatch people intents over the engine`.

### Task 3: the DAV account extension + host/ipc
**Files:** `crates/astra-account/src/dav.rs` (+ tests; the config section, the connect path dialing BOTH legs); `client/desktop/src/{boot.rs,commands/people.rs}` (+ tests; the boot installs every dialed leg; the people pull, person detail, suggestions, and CRUD commands); `client/web/src/services/people.ts` (+ tests; the wire mirrors).
- [ ] Tests first; implement; commit `feat: dial the dav legs and host the people commands`.

### Task 4: the people UI + autosuggest
**Files:** `client/web/src/features/people/**` (the view switch entry, the list with sections and the disclosure, the detail, the editor with the card choice) + tests; the composer autosuggest (the token completion, the pills, the floating list, the caret rules) + tests; avatars (the monogram + the photo fetch) on the contacts row and the reading header.
- [ ] Tests first per D1/D3-D5 UI list; implement; commit `feat: render people, autosuggest, and avatars`.

### Task 5: live proof + docs
**Files:** the gated live people tests (the CardDAV sync read, the create/edit round trip, the cross-account joined person with the live `In 2 accounts` disclosure, autosuggest over the seeded sent history); `docs/contacts.md` + zh; capabilities + zh (the contacts, autosuggest, avatar, and the calendar-host rows); the fragment; the full battery.
- [ ] Commit `feat: prove people live and record the contracts`.

### Task 6: close-out
- [ ] Full battery; branch `sp2i-contacts` pushed; PR (squash expected); acceptance evidence; ledger cleanup.

## Self-review notes

- Spec coverage: D1 (T1 types/T4 UI), D2 (T1/T2), D3 (T2/T4), D4 (T2/T4), D5 (T2 suggest + T4 composer), D6 (T3), D7 (T4), live + docs (T5).
- Interface chain: the survey (T1) binds everything; types (T1) -> dispatch (T2) -> DAV/host (T3) -> UI (T4) -> live + docs (T5).
- Risks: the engine's people model breadth (the survey decides what ports now vs defers, never silently); the composer autosuggest touches the existing recipient fields (the pills refactor must keep the one-string contract the wire pins); the DAV extension touches boot (the calendar surface un-gap must not regress the mail dial); Stalwart CardDAV (verify; the fallback is the recorded gap).
