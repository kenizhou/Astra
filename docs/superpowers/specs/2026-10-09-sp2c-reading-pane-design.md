# SP2c Reading Pane Design

Date: 2026-10-09
Status: proposed
Parent spec: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (sections 5, 15)

## 1. Purpose

Read a message: fetch the body through the engine, sanitise the HTML with
mail-calendar's rendering-security posture, resolve inline CID images, gate
remote images behind an explicit toggle, list attachment metadata, and mark
the message read on open. The reading pane replaces its placeholder.

## 2. Decisions

- **D1**: a new `Surface::Reading` carrying `ReadingSnapshot` (message ref,
  subject, from display, date, `html: Option<String>` sanitised, `plain:
  Option<String>`, `attachments: Vec<AttachmentRow>` with id, file_name,
  media_type, size, `remote_image_count`) in astra-viewmodel. One reader
  (the pane) in SP2c: a single `Surfaced<Option<ReadingSnapshot>>`, no
  per-reader map (mail-calendar's `SurfacedMap` and window readers return
  with the multi-window slice; recorded deviation).
- **D2**: two new Intents: `OpenMessage { message: MessageRef }` (the pane
  reader is implied; MC's `ReaderId` parameter is the recorded deviation)
  and `CloseReading`. Opening marks the message read through the existing
  write path and publishes the snapshot; `RefreshMail` republishes the open
  reading if the message still exists and closes the surface if it is gone.
- **D3**: the body pipeline runs in Rust: engine `message_source` decode ->
  sanitise with ammonia mirroring MC's sanitiser configuration (read MC's
  reading path and its rendering-security contract; ammonia 4 joins the
  workspace table) -> inline CID images become `data:` URIs decoded from
  the engine's attachment parts, skipped with alt text over a size cap ->
  remote `src` rewritten to a `data-astra-src` placeholder with the count
  surfaced, so nothing remote loads without the toggle. The plain
  alternative passes through untouched.
- **D4**: the SPA renders the sanitised HTML in a sandboxed iframe
  (`sandbox` without `allow-same-origin`, `csp` limiting `img-src` to
  `data:`), srcdoc-delivered; the "show remote images" toggle rewrites
  `data-astra-src` to `src` client-side per message; attachments render as
  read-only metadata rows; plain text renders when no HTML exists; an empty
  state renders when nothing is open.
- **D5**: bytes never cross as files in SP2c: inline images ride inside the
  HTML string as data URIs, attachment payloads are metadata only (saving
  arrives with the attachments slice).

## 3. Testing

- Unit (astra-app): sanitisation vectors ported from MC's reading tests
  (script strip, style confinement, link policy); CID resolution (inline
  part becomes data URI; oversized inline skipped); remote placeholder
  rewrite and count; open-marks-read; refresh republish and close-on-gone.
- Wire: ReadingSnapshot literals pinned both sides; new intent literals
  pinned both sides; `surface_changed` carries `reading`.
- Frontend (vitest): sandboxed iframe renders the sanitised HTML; remote
  images absent until the toggle and present after; attachment rows; plain
  fallback; empty state; opening from the list dispatches `open_message`.
- Live (gated, harness): a rich seeded message (HTML body, one remote
  image, one inline CID image, one attachment) in the Projects folder; the
  gated test asserts the sanitised output, blocked remote count, data-URI
  CID, attachment row, and the read-on-open transition.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build and vitest
   green.
2. Opening a seeded message renders sanitised HTML with remote images
   blocked until toggled, the inline CID visible, attachment metadata
   shown, and the message marked read (unread count drops, survives
   RefreshMail).
3. The reading surface closes on CloseReading and on message removal.
4. `docs/capabilities.md` rows move (reading to partial, rendering security
   to partial) with the zh mirror and a changelog fragment in the same
   change.
5. Additive wire only: existing fixtures and tests pass unchanged.
