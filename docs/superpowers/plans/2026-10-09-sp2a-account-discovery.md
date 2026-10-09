# SP2a Account Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port mail-calendar's autodetection into `astra-autodetect`, expose `detect_account_settings` from the host, wire the wizard to detect first with manual fallback, and prove detection live against the harness's JMAP endpoint.

**Architecture:** A leaf crate `astra-autodetect` holds the strategies (HTTP probes plus DNS lookups); `astra-account` gains the recommendation layer (`recommend`, `SetupChoices`); the host adds one setup-path command; the SPA wizard calls it before showing the manual form. No Intent or dispatch changes.

**Tech Stack:** Rust (existing workspace lints), reqwest via the workspace table (as MC's autodetect uses), hickory-resolver or MC's actual DNS crate (read MC first and mirror), React wizard extension.

**Spec:** `docs/superpowers/specs/2026-10-09-sp2a-account-discovery-design.md`

## Global Constraints

- Same as the SP1 plan: SPDX headers, 500-line cap with MC-style splits, AR1-AR4 adaptation rules (astra renames, no history comments, no uniffi, engine types via re-exports, time crate), deny-everything lint set, gate green before every commit, conventional commits, no dashes, British English, zh mirrors move with their English sources.
- Email addresses never appear in URLs (account-autodetect contract); untrusted (non-HTTPS) sources require the approval gate exactly as MC implements it.
- Port source root MC = D:/Projects/mailclient/opensource/mail-calendar.

## Adaptation rule additions

- **AR7**: HTTP probing uses the workspace reqwest with MC's feature set; DNS uses whatever crate MC's autodetect pins (mirror it into the workspace table); timeouts follow MC's constants.

---

### Task 1: astra-autodetect crate

**Files:**
- Create: `crates/astra-autodetect/**` (mirror MC `crates/mailcal-autodetect/src` module layout: jmap probe, autoconfig, ispdb, srv, mx, plus the untrusted-source gate and the public `detect` entry)
- Port from: `MC/crates/mailcal-autodetect/**` with its test files.

**Interfaces:**
- Produces: `detect(email, options) -> DetectedSettings` (async; strategies in MC's order with MC's precedence), `DetectedSettings` and per-strategy outcome types with serde where they cross IPC, `UntrustedSource` approval gating types.

- [ ] Step 1: Port MC's test files first where they are strategy-level (parsers, precedence); run red against the empty crate.
- [ ] Step 2: Port the modules per AR1-AR4 + AR7; drop nothing without recording it (MC ships only these strategies today).
- [ ] Step 3: `cargo test -p astra-autodetect` green; clippy/fmt/gate green.
- [ ] Step 4: Commit `feat: port the account autodetect strategies from mail-calendar`.

### Task 2: recommendation layer in astra-account

**Files:**
- Create: `crates/astra-account/src/{autodetect.rs, setup_choices.rs}` + tests
- Port from: `MC/crates/mailcal-account/src/{autodetect.rs, setup_choices.rs}`.

**Interfaces:**
- Produces: `recommend(email, detected, routes) -> SetupRecommendation` (variants Imap | Dav | Microsoft | Google | Jmap | Manual), `SetupChoices` (the toggle set the wizard renders), both serde for IPC.

- [ ] Step 1: Port MC's tests (recommendation matrix per detection outcome, choice rendering).
- [ ] Step 2: Port the modules; astra-account depends on astra-autodetect.
- [ ] Step 3: Tests/clippy/fmt/gate green; commit `feat: port the setup recommendation and choices layer`.

### Task 3: host command and wire types

**Files:**
- Modify: `client/desktop/src/commands.rs` (+ tests), `client/web/src/services/{ipc.ts, surfaces.ts}` (+ tests)
- Port from: `MC/crates/mailcal-bindings/src/autodetect.rs` (the detect_account_settings shape).

**Interfaces:**
- Produces: `detect_account_settings(email) -> RecommendationView` command (five second timeout, typed error); IPC types in the SPA mirroring the Rust serde wire (test-pinned literals both sides).

- [ ] Step 1: commands_tests: unknown domain yields manual-IMAP recommendation matching the manual defaults; timeout path returns a typed error, never a panic.
- [ ] Step 2: Implement + register the command; ipc.ts wrapper + wire tests.
- [ ] Step 3: All green; commit `feat: add the detect_account_settings host command and ipc wire`.

### Task 4: wizard integration

**Files:**
- Modify: `client/web/src/features/account-setup/**` (+ tests)

- [ ] Step 1: Tests first: entering an email triggers detect; recommendation renders the choices; manual toggle shows the form pre-filled with the recommended/manual defaults; detect failure or timeout falls back to manual silently noted in the UI.
- [ ] Step 2: Implement the detect step in AccountSetupFlow (loading state, error surface, five second client-side guard too).
- [ ] Step 3: vitest + build + gate green; commit `feat: wire account discovery into the setup wizard`.

### Task 5: live JMAP detection and docs

**Files:**
- Modify: `scripts/dev/harness.sh` (env gains JMAP URL), `crates/astra-app/src/tests_live_harness.rs` or a sibling in astra-autodetect for the gated JMAP detection test
- Modify: `docs/capabilities.md` + zh mirror, `docs/changelog/unreleased/account-discovery.md`

- [ ] Step 1: Read how MC's harness exposes JMAP; extend compose/env accordingly (port mapping on loopback).
- [ ] Step 2: Gated test: for astra1@example.test the JMAP strategy returns a JMAP recommendation (skips without ASTRA_LIVE_HARNESS=1).
- [ ] Step 3: Run `bash scripts/dev/harness.sh up`, the gated test green, slice-check still PASS; record outputs.
- [ ] Step 4: capabilities rows (auto-detect -> partial; wizard row notes discovery) + zh mirror + changelog fragment (Platforms: desktop, Bump: minor).
- [ ] Step 5: Full battery; commit `feat: prove live jmap detection and record the capability`.

### Task 6: close-out

- [ ] Step 1: Full battery green; branch `sp2a-account-discovery` pushed; PR opened linking the spec; the PR body notes the SP1 deferral discharged.
- [ ] Step 2: Ledger cleanup per the SDD skill; acceptance criteria evidence per spec section 4.

## Self-review notes

- Spec coverage: D1 (Task 1), D2 (Task 2), D3 (Task 3), D4 (no Intent change anywhere; Task 3 registers one setup-path command), D5 (Task 5). Acceptance criteria 1-5 map to Tasks 1-6.
- Interface chain: DetectedSettings (Task 1) -> recommend (Task 2) -> RecommendationView + ipc wire (Task 3) -> wizard (Task 4) -> live proof (Task 5).
