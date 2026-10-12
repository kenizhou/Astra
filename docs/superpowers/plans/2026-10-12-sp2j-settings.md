# SP2j Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver settings end to end: the Settings surface with the snapshot, the intent family persisted into the preferences document, the core's consumption of its settings (zone, first day, horizon, default calendar), the Accounts page with the dav editor and remove-account, About, the live dav-editor proof, and the contract doc.

**Architecture:** astra-app owns the SettingsIntent dispatch and the persistence; the preferences document is the one store (TOML, additive); the accounts page reads the host's account configs; the web renders the screen.
**Tech Stack:** Existing workspace; no new dependencies.
**Spec:** `docs/superpowers/specs/2026-10-12-sp2j-settings-design.md`

## Global Constraints

- House rules as every slice: SPDX, 500-line cap, deny-everything lints, gate before commit (direct exit), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- Standing: AUTO-CONTINUE the moment the PR merges; SQUASH AND MERGE; the 10-minute author window.
- Settings the core does not consume are host chrome; settings it does consume flow through the app state, never a second source.

---

### Task 1: the types + persistence
**Files:** `crates/astra-app/src/protocol/intent_settings.rs` + tests; `crates/astra-app/src/preferences.rs` (+ tests; the settings fields, additive with defaults); `crates/astra-viewmodel/src/settings.rs` + tests (the SettingsSnapshot shapes: the settings block, the account rows, the about block).
- [ ] Tests first; implement; commit `feat: add the settings protocol and preferences fields`.

### Task 2: the dispatch + consumption
**Files:** `crates/astra-app/src/settings/{mod.rs, accounts.rs}` + tests (the dispatch arms over the persisted preferences; the calendar's zone/first-day/horizon/default-calendar consumption; the view_mode consolidation; the dav editor's re-dial-and-install with the failed-keep-old contract; remove account).
- [ ] Tests first per D2-D4 unit list; implement; commit `feat: dispatch settings intents and consume them`.

### Task 3: host/ipc
**Files:** `client/desktop/src/commands/settings.rs` (+ tests; the settings pull, the about block, the zone validation seam); `client/web/src/services/settings.ts` (+ tests + the shared fixture pinned both languages).
- [ ] Tests first; implement; commit `feat: host the settings commands and wire`.

### Task 4: the UI
**Files:** `client/web/src/features/settings/**` (the modal screen, the source list, the General/Calendar/Reading/Composing/Accounts/About details, the pickers, the dav editor, the remove confirmation) + tests; the TitleBar entry; the theme/clock consumption in the chrome.
- [ ] Tests first per D1/D4/D6 UI list; implement; commit `feat: render the settings screen and the accounts page`.

### Task 5: live proof + docs
**Files:** the gated live settings test (the dav editor against the harness: edit, re-dial, both legs install, the calendar and contacts fill; a failed re-dial keeps the old legs); `docs/settings.md` + zh; capabilities + zh; the fragment; the full battery.
- [ ] Commit `feat: prove the dav editor live and record the contract`.

### Task 6: close-out
- [ ] Full battery; branch `sp2j-settings` pushed; PR (squash expected); acceptance evidence; ledger cleanup.

## Self-review notes

- Spec coverage: D1 (T1 types/T4 UI), D2 (T1/T2), D3 (T2), D4 (T2/T4), D5 (T1/T3/T4), D6 (T2/T4), live + docs (T5).
- Interface chain: types + persistence (T1) -> dispatch/consumption (T2) -> host (T3) -> UI (T4) -> live + docs (T5).
- Risks: the preferences file format (additive with defaults; an old file must load clean: pin it); the dav re-dial touching live account state (the failed-keep-old contract must not tear down working legs); the view_mode consolidation (the existing list arm and its wire must not break: additive delegation); remove account's teardown breadth (the store, the legs, the preferences' account entries, the credential store).
