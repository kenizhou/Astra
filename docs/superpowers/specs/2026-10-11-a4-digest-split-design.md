# A4 Digest, Analytics, Split Inbox Design

Date: 2026-10-11
Status: proposed
Parent spec: `docs/superpowers/specs/2026-08-...-workflow-design` (section 9.3, A4)
Consumes: the A1-A3 skeleton (client, judges, seams, ledgers, eval harness, the incoming pass's categorisation results), the reading pane, the message list.

## 1. Purpose

The last inbox-zero AI catalogue items that are mail-only: an on-demand
digest of recent mail, an analytics report over the recent window, and the
FYI/Actioned split inbox derived from the stored categorisation. Meeting
briefs defer to the calendar feature slice (their data dependency) and are
recorded as a known deferral, never silent.

## 2. Decisions

- **D1 (digest)**: migration `0010_ai_documents` (id, account, kind:
  digest | report, period_start, period_end, content, created_at,
  correlation_id). `digest_generate(account, days?)` (default 7, capped
  31) collects the window's messages (the existing newest-first reads),
  builds the category rollup from the STORED ai_results categorisation
  where present plus per-category message lists, and makes ONE summarising
  generation (`DIGEST_PROMPT_VERSION`, DEFAULT tier; the messages
  untrusted-wrapped, capped per category head-five). The digest stores
  with its period; regeneration replaces the same-period row. The reading
  surface gains nothing; a Digest panel (a dialog or pane section,
  picker's choice recorded) renders the latest digest with a Generate
  button; the CLI reads.
- **D2 (analytics report)**: `report_generate(account, days?)` produces a
  three-section report (executive summary, response patterns,
  recommendations) over the same window with computed statistics (counts
  per category, unread rate, sent count) feeding ONE structured generation
  (`REPORT_PROMPT_VERSION`, DEFAULT tier). Stored beside digests (kind
  report). Same panel with a tab; the CLI reads.
- **D3 (split inbox)**: NO new model calls. The split derives from the
  stored categorisation stage results: Actioned = primary; FYI =
  newsletter, notification, receipt, cold; anything without a stored
  category appears in All only. The message list gains a three-way toggle
  (All / Actioned / FYI) filtering locally from the split map the host
  serves per account (`split_map(account) -> {key -> Actioned|FYI}`
  pulled alongside the snapshot; additive). The mapping is pinned data,
  versioned as `SPLIT_MAPPING_VERSION` (a mapping change is an eval
  rerun).
- **D4 (meeting briefs)**: deferred to the calendar slice; the
  capabilities row stays not-started with the dependency named.

## 3. Testing

- Unit: digest collection (the window cap, the category rollup from
  stored rows, the per-category head-five, the untrusted wrap); report
  stats computation; the same-period replace; the split mapping (every
  category word mapped or excluded, pinned); the eval fixtures for the
  two new constants + the mapping version.
- Fixture servers: the DEFAULT tier shapes.
- Frontend: the panel (generate/loading/render for both kinds, the
  period label); the split toggle (filters, the count badges, the
  uncategorised-All-only rule).
- Live (gated): a real digest over the seeded notes; a real report; the
  split map over the pass's real results; costs sane.

## 4. Acceptance criteria

1. Gate, workspace tests, cargo doc, desktop build, web build, vitest,
   sidecar tests green.
2. Offline: digest and report lifecycles pinned (generate -> store ->
   render; regenerate replaces); the split mapping pinned end to end.
3. Live: digest + report run against the endpoint with sane costs; the
   split map serves over the harness's real categorisation results.
4. `docs/ai-digest.md` (+ zh) with the contracts (window caps, the
   rollup source, the replace semantics, the split mapping, the meeting-
   briefs deferral), capabilities rows moved with mirrors, fragments,
   eval fixtures with version pins.
5. Additive wire only; existing fixtures and tests pass unchanged.
