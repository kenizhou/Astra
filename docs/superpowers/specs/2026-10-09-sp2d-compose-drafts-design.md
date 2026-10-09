# SP2d Compose and Drafts Design

Date: 2026-10-09
Status: proposed
Parent spec: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (sections 5, 15)

## 1. Purpose

Write mail and not lose it: port the composer document model, a trimmed
TipTap editor, the drafts family (save, discard, close) over the engine's
server-side drafts, idle autosave, resume, and a draft status surface.
Reply and forward (quoting, sender prefill) arrive with the sending slice;
this slice opens from a New message action and from resume of an existing
draft.

## 2. Decisions

- **D1**: port `mailcal-composer` as `crates/astra-composer`
  (ComposerDocument with serde as MC ships it, deterministic HTML and plain
  text rendering, the attachment and inline CID manifest) trimmed to what
  the SP2d editor produces; the composer-security posture (no scripts in
  generated HTML) carries over from MC's contract.
- **D2**: the drafts family ports as MC shapes: `DraftsIntent::{Save,
  Discard, Close}` with `CompositionId` newtype; save writes through the
  engine's `put_draft` keyed for server-side storage, discard through
  `delete_draft`; autosave runs core-side after thirty seconds of
  composition idleness (tokio time; astra-app may use sync plus time only).
- **D3**: a new `Surface::DraftStatus` (non-debounced) carrying a minimal
  status snapshot (composition id, state: saving or saved, saved-at
  instant); the SPA pulls it to badge the composer.
- **D4**: the SPA composer uses TipTap (kylins' choice) with a trimmed
  extension set (paragraph, bold, italic, underline, bullet and ordered
  lists, links, hard breaks); the editor state converts to and from the
  ComposerDocument JSON wire; To, Cc, Bcc, and Subject are plain fields;
  From is the default send account (read-only this slice).
- **D5**: initiation: a New message action opens a fresh composition with a
  minted CompositionId; on app start, an existing draft for the default
  account resumes into the composer (MC's resume-into-composer behaviour,
  single draft scope this slice); close saves then closes; discard deletes.

## 3. Testing

- Unit (astra-composer): document JSON round trips; HTML and text
  rendering determinism vectors from MC's tests; the manifest shape.
- Unit (astra-app, in-memory engine + fakes): save persists (store read
  back), autosave fires after idle and not before, discard removes, close
  saves then clears status, resume path restores content; DraftStatus
  publishes on each transition.
- Wire: ComposerDocument, DraftsIntent, and the status snapshot literals
  pinned both sides.
- Frontend (vitest): editor round trip to and from the document wire;
  autosave dispatches Save after idle (fake timers) and not on every
  keystroke; the badge reflects the status surface; New message mints a
  composition; resume hydrates fields and content; discard and close
  dispatch correctly.
- Live (gated, harness): save a draft through dispatch, assert the engine
  store holds it; discard clears it; restart path resumes it.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build and vitest
   green.
2. A draft written in the composer autosaves after idle, survives an app
   restart (resume restores fields and content), and discard removes it
   (gated live proof for the store round trip).
3. The status badge reflects saving and saved transitions.
4. `docs/capabilities.md` rows move (compose partial, drafts partial) with
   the zh mirror and a changelog fragment in the same change.
5. Additive wire only; existing fixtures and tests pass unchanged.
