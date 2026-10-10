# A2 Reply Drafting and Summarisation Design

Date: 2026-10-10
Status: proposed
Parent specs: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (section 9.3), `docs/superpowers/specs/2026-10-10-binary-judge-design.md` (JudgeDraftSurface)
Consumes: the A1 skeleton (client, judge, seams, ledgers), the SP2c reading pipeline, the SP2d/SP2f composer and drafts.

## 1. Purpose

The reading experience gains its AI layer: background reply drafts surfaced as
smart-reply chips, a cached summarise action, and a per-account writing-style
profile that shapes the drafts. Accepting a chip opens the composer prefilled
as a reply; the human still decides and sends.

## 2. Decisions

- **D1 (draft generation)**: opening a message (OpenMessage) arms a
  background DraftReply generation (DRAFT tier, `DRAFT_REPLY_PROMPT_VERSION`)
  with a context collector: the original message plus the thread's recent
  history (the engine's thread reads, capped), the per-account style profile
  when present, and the untrusted-wrapped mail content. The structured
  output carries `{reply, confidence}` (inbox-zero's shape); results land in
  a draft pool (migration `0004_draft_pool`: message key, reply, confidence,
  created_at, correlation id). Generation failure records a skipped row; it
  never disturbs the reading surface.
- **D2 (surfacing gate)**: a pool entry surfaces as a chip only when its
  confidence clears a constant AND JudgeDraftSurface answers Yes (nano tier;
  the question asks whether the draft is a genuinely useful reply the user
  could send, not filler). Abstain-default No keeps the chip hidden (fail
  quiet, nothing lost: the pool row remains queryable via the CLI).
- **D3 (smart-reply chips)**: the reading pane shows up to two cleared chips
  (short previews, trimmed). Accept dispatches the SP2f reply flow with the
  draft text as the document body (the composer opens as a reply, human
  edits and sends through the durable outbox as ever). Dismiss hides the
  chips for that message (a `chips_dismissed` marker keyed by message).
- **D4 (summarisation)**: `summarise(message)` runs the Summarise use case
  (DEFAULT tier, `SUMMARISE_PROMPT_VERSION`) with the untrusted-wrapped body;
  the result caches in migration `0005_summarise_cache` keyed by message key
  + body hash (invalidation on change); cache hits return without a call.
  The reading pane gains a Summarise button rendering the cached-or-fresh
  summary in a collapsible block.
- **D5 (style profile)**: `StyleInfer` (new use case, ECONOMY tier) builds a
  per-account style note from up to twenty recent Sent messages (the
  engine's Sent-folder reads; a structured call returning a short
  description: tone, formality, sign-off habits). Profiles cache in
  `0004`'s sibling table `style_profiles` (account PK, note, built_at) and
  rebuild only on demand (a CLI verb `ai style rebuild` and a boot-time
  once-per-day refresh at most). No Sent mail, no profile: the draft prompt
  omits the note. Mail content in the infer prompt is untrusted-wrapped;
  the note itself is model-derived text treated as data in the draft prompt.
- **D6 (wire)**: host commands `draft_chips(account, key)`, `summarise(account,
  key)`, `style_profile(account)`; a `draft_pool` refresh event when a pool
  row lands (the reading pane pulls chips on open and on the event). The
  CLI gains read verbs `ai drafts <account> <key...>` and `ai summary
  <account> <key...>` plus `ai style rebuild` (the one mutating verb,
  `--preview` describing the rebuild).

## 3. Testing

- Unit (astra-app, fakes + fixture seams): generation shape (structured
  `{reply, confidence}`, repair path, failure -> skipped row); the gate
  (confidence threshold, judge Yes/No/abstain-default); chips data (top two,
  trim, dismissal marker); summarise cache (hit/miss/invalidate-on-hash);
  style infer (shape, no-Sent no-profile, the daily cap); correlation
  discipline (one id per generation; ledger rows).
- Fixture servers: the DRAFT and DEFAULT tier wire shapes.
- Frontend (vitest): chips render from the pull; accept dispatches the
  reply flow with the draft document; dismiss marks; the summarise button
  (loading, cached instant, fresh, error surfaces); the collapsible block.
- Live (gated, the provided endpoint + harness): a real draft generated for
  the seeded rich note (pool row + ledger cost); a real judge gate decision;
  a real summarise (cached second call asserts no second ledger row); the
  style infer against astra1's Sent (post-send copies exist from the mail
  suite's sends); chips end-to-end through the host commands.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build, vitest,
   sidecar tests green.
2. Offline: the full chip lifecycle (generate -> gate -> chip -> accept ->
   composer prefilled as reply); cache behaviour pinned; dismissal pinned.
3. Live: draft + judge + summarise + style run against the endpoint with
   sane costs; the cached summarise second call makes no network request.
4. `docs/ai-replies.md` (+ zh) with the contracts (pool, gate, chips, cache,
   style, the human-sends rule), capabilities rows moved with mirrors,
   fragments.
5. Additive wire only; existing fixtures and tests pass unchanged.
