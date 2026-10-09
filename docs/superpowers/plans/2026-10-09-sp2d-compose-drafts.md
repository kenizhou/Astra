# SP2d Compose and Drafts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver compose and drafts end to end: the astra-composer document model, the drafts family over engine server-side drafts with idle autosave and resume, the DraftStatus surface, a trimmed TipTap composer with wire round trip, live store proof.

**Architecture:** A leaf crate astra-composer (serde document, deterministic rendering, manifests); astra-app gains the Drafts family, a drafts module over engine put/delete_draft, an idle autosave task, and the DraftStatus surface; the SPA composer converts TipTap state to and from the ComposerDocument wire and badges from the status surface.

**Tech Stack:** Existing workspace; TipTap v3 (kylins' version) with the trimmed extension set in client/web; engine drafts writes at the pinned rev.

**Spec:** `docs/superpowers/specs/2026-10-09-sp2d-compose-drafts-design.md`

## Global Constraints

- House rules as prior slices: SPDX, 500-line cap, deny-everything lints, gate before commit (never piped), conventional commits, no dashes, British English, zh mirrors same-commit, additive wire only.
- Port roots: MC = D:/Projects/mailclient/opensource/mail-calendar (mailcal-composer, mailcal-app draft_ops, mailcal-account drafts); KY = D:/Projects/mailclient/kylins (TipTap setup); engine via `git show 68eab00:<path>`.
- AR1-AR7 apply.

---

### Task 1: astra-composer crate

**Files:**
- Create: `crates/astra-composer/**` mirroring MC `mailcal-composer/src` trimmed (types.rs with ComposerDocument/Blob manifest, render paths for deterministic html and text) + tests
- Port from: MC `crates/mailcal-composer/**`.

**Interfaces:**
- Produces: `ComposerDocument` (serde as MC ships; the block model MC defines) plus whatever minimal rendering API the drafts round trip needs this slice (full deterministic rendering vectors from MC's tests; the sending slice consumes the render API later; if render entry points exceed SP2d needs, port them anyway with their tests since they are the crate's purpose, recording any trim).

- [ ] Step 1: Port MC's composer tests first (document JSON round trips, rendering determinism vectors, manifest shape); red on the empty crate.
- [ ] Step 2: Port the modules per AR1-AR4 (astra renames, no uniffi, engine types via re-exports where MC used them, time crate).
- [ ] Step 3: Tests/clippy/fmt/gate green; commit `feat: port the composer document crate from mail-calendar`.

### Task 2: drafts family, autosave, and status surface in astra-app

**Files:**
- Modify: `crates/astra-app/src/{protocol/intent.rs, protocol.rs, lib.rs}` + protocol tests; Create: `crates/astra-app/src/{drafts.rs, drafts_tests.rs}`; Modify `Cargo.toml` (astra-composer dep) and workspace members list if enumerated
- Port from: MC `mailcal-app/src/{draft_ops.rs (family dispatch), protocol/intent_drafts.rs}` + MC `mailcal-account` drafts helpers; engine put/delete_draft at the pinned rev.

**Interfaces:**
- Produces: `DraftsIntent::{Save{composition, from, to, cc, bcc, subject, document, then_close}, Discard{composition}, Close{composition}}` (MC shapes; blobs deferred to the attachments slice, recorded); `CompositionId` newtype (blank-rejecting per MC); dispatch family arm; drafts module: save via engine `put_draft` (READ the signature at 68eab00 first), discard via `delete_draft`, resume read (latest draft for an account), idle autosave (thirty seconds, tokio time, cancellation on close/discard); `Surface::DraftStatus` (event `draft_status`, non-debounced) with a minimal status snapshot type in astra-viewmodel (composition, state, saved_at) and its Surfaced cell.

- [ ] Step 1: protocol wire tests first (three variants, CompositionId rejection).
- [ ] Step 2: drafts_tests first (in-memory engine + fakes): save persists and reads back; autosave fires after idle not before (tokio time pause or short constant seam); discard removes; close saves then clears status; resume restores; status publishes on transitions.
- [ ] Step 3: Implement (status snapshot type + wire tests in viewmodel alongside).
- [ ] Step 4: All green; commit `feat: add the drafts family with idle autosave and status surface`.

### Task 3: host command and IPC wire

**Files:**
- Modify: `client/desktop/src/{commands.rs, app_state.rs if the pull list grows}` + a sibling test file; `client/web/src/services/{ipc.ts, ipc.test.ts}`; `client/web/src/events/surfaceEvents.ts` (+ tests)

**Interfaces:**
- Produces: `draft_status()` pull command; surface union gains `draft_status` with its pull; ipc.ts gains ComposerDocument + DraftsIntent + status types (literals mirrored from Rust tests).

- [ ] Step 1: commands tests first (pull reflects transitions; None when idle-cleared).
- [ ] Step 2: Implement + register; wire tests both sides; surfaceEvents extended (reading precedent).
- [ ] Step 3: All green; commit `feat: expose drafts and the status surface over ipc`.

### Task 4: the composer in the SPA

**Files:**
- Create: `client/web/src/features/composer/{Composer.tsx, ComposerFields.tsx, Editor.tsx, documentWire.ts, useAutosave.ts}` + tests; Modify `client/web/src/components/layout/AppShell.tsx` (New message action + composer pane), `client/web/package.json` (TipTap v3 + trimmed extensions at KY's versions)
- Port from: KY `kylins.client.frontend` composer/TipTap wiring; MC field semantics.

**Interfaces:**
- Produces: documentWire.ts (TipTap state <-> ComposerDocument JSON both ways, deterministic); useAutosave (idle thirty seconds, resets on edit, dispatches Save; skipped when unchanged); Composer (To/Cc/Bcc toggle, Subject, From read-only, editor, badge from the status surface, Discard and Close buttons); New message mints a composition; app-start resume hydrates from the engine draft (via a resume pull command or the drafts read through dispatch; pick the honest seam, likely a `draft_resume()` pull command added in this task and tested).

- [ ] Step 1: Tests first per the brief's list (round trip both ways incl. lists/links/breaks; autosave timing with fake timers; badge transitions; New message mints; resume hydrates; discard/close dispatch with exact payloads).
- [ ] Step 2: Implement; npm install (TipTap adds no postinstall; allowScripts untouched).
- [ ] Step 3: vitest + build + gate green; commit `feat: add the composer with autosave and resume`.

### Task 5: live store proof and docs

**Files:**
- Modify: `crates/astra-app/src/tests_live_harness.rs` (+ drafts test); `docs/capabilities.md` + zh mirror; `docs/changelog/unreleased/compose-drafts.md`

- [ ] Step 1: Gated drafts test: connect astra1 -> Save a document via dispatch -> assert the engine store holds the draft (store read or resume path) -> Discard -> assert gone -> Save again -> the resume path returns content.
- [ ] Step 2: Run live: harness up, gated green, slice-check PASS; record outputs.
- [ ] Step 3: capabilities rows (compose -> partial, drafts -> partial) + zh mirror + fragment (Platforms: desktop, Bump: minor).
- [ ] Step 4: Full battery; commit `feat: prove drafts against the live store and record the capability`.

### Task 6: close-out

- [ ] Step 1: Full battery green; branch `sp2d-compose-drafts` pushed; PR opened linking the spec; acceptance evidence; ledger cleanup.

## Self-review notes

- Spec coverage: D1 (T1), D2 (T2), D3 (T2 status surface + T3 ipc), D4 (T4), D5 (T4 initiation/resume), live proof + docs (T5).
- Interface chain: ComposerDocument (T1) -> drafts family (T2) -> ipc wire (T3) -> composer (T4) -> live proof (T5).
- Risks carried: engine put/delete_draft signatures at 68eab00 (T2 step 3 names it); TipTap extension set trim vs document model fidelity (T4 pins round trips; MC document blocks outside the trimmed set must round trip as-is or be rejected loudly, not silently dropped; pin that).
