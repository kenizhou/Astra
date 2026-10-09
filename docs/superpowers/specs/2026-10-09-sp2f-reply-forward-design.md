# SP2f Reply and Forward Design

Date: 2026-10-09
Status: proposed
Parent spec: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (sections 5, 15)

## 1. Purpose

Answer mail: Reply, Reply all, and Forward from the reading pane, with
quoted content, subject and recipient prefill, and the send path reusing
SP2e's durable outbox. This closes the core mail loop (read, write, reply,
send) that every later slice builds on.

## 2. Decisions

- **D1**: `Intent::SubmitRichReply { from, to, cc, in_reply_to, subject,
  document, blobs }` and `Intent::SubmitRichForward { from, to, subject,
  document, blobs }` port in MC's shapes (in_reply_to carries the original
  Message-ID header; forward has none); both submit through the same
  durable-outbox path as SubmitRichMail, threading headers set by the
  engine's draft builder per MC's contract.
- **D2**: reply quoting renders in Rust: the reading snapshot's plain text
  (or html-to-text fallback) becomes a Quote block in the ComposerDocument
  with the original's attribution line (sender and date, MC's format); the
  quote style follows the account preference when the settings slice lands
  (constant default this slice, recorded). Forward packs the original body
  as an attributed Quote plus, when the original has attachments, forward
  carries them as blobs (attachment re-send is the engine's raw-message
  path in MC; if the pinned engine's forward-with-attachments needs the
  staging flow, attachments defer to that slice and the deferral is
  recorded in capabilities).
- **D3**: a `ComposeRequest`-shaped initiation: the reading pane's Reply,
  Reply all, and Forward actions dispatch an intent family
  `Compose::{Reply{message}, ReplyAll{message}, Forward{message}}` (new
  family variant alongside Folders/Drafts/Outbox) whose arm computes the
  prefilled composition server-side (recipients from the original's headers
  via the engine's message read, quote per D2) and publishes it on a
  `Surface::ComposeRequest` snapshot (non-debounced) the composer hydrates
  from; MC's ComposeRequest surface shape, single-slot.
- **D4**: the composer gains hydrate-from-compose-request (fields + quote
  document + in_reply_to riding the draft save so a saved reply resumes
  correctly); subject prefill follows MC (Re:/Fwd: prefixes, original
  subject preserved); Reply all fills cc from the original's cc minus self.
- **D5**: plain-text quoting only this slice (html originals quote through
  their plain alternative when present, else a sanitised-text fallback);
  rich quote rendering (nested html quote blocks) defers to the rich-paste
  slice, recorded.

## 3. Testing

- Unit (astra-app, in-memory engine + fakes with full headers): Reply
  prefills to = original sender (list-unquote per MC if trivially
  available), Reply all adds cc = original cc minus self and to minus self;
  Forward prefills empty to; subjects prefixed Re:/Fwd: per MC rules;
  in_reply_to set for replies; the quote block carries the attribution and
  body; submit paths thread correctly (engine fakes record the draft's
  headers).
- Wire: the two submit intents + the Compose family + the ComposeRequest
  snapshot literals pinned both sides.
- Frontend (vitest): reading pane buttons dispatch the Compose intents with
  exact payloads; the composer hydrates fields + quote document from the
  request surface; subject prefixes displayed; Send dispatches
  SubmitRichReply/Forward with in_reply_to.
- Live (gated, harness): reply to a seeded note from astra2 arrives back at
  astra1 (threaded: the reply's In-Reply-To matches the original's
  Message-ID, assertable via the engine's thread read or headers); forward
  delivers to the other account with the Fwd: subject.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build and vitest
   green.
2. Live: a reply from astra2 to astra1's seeded note delivers and carries
   the threading header; a forward delivers with the prefixed subject; both
   ride the durable outbox (queue empties).
3. Reply all computes recipients per MC rules (self excluded); quote
   attribution present.
4. `docs/capabilities.md` rows move (reply/forward to partial; threading
   partial for reply linkage) with the zh mirror, a changelog fragment, and
   the reply/forward section added to `docs/sending.md` (or a sibling
   compose doc if MC splits it) in the same change.
5. Additive wire only; existing fixtures and tests pass unchanged.
