# AI replies: the Astra Mail contract

**Scope.** The A2 reply drafting features on the A1 foundation: the draft
pool and its generation, the surfacing gate and the smart-reply chips, the
cached summarise action, the per-account writing-style profile, the host
commands and CLI reads that expose them, and the eval pins that hold their
prompts. Binding on every surface that ships any of it (today the desktop
host, the web reading pane, and `astra-cli`).

**Principle.** *AI drafts; the human decides and sends.* A drafted reply is
a suggestion the reader accepts, edits, or dismisses; nothing here sends
anything. Accepting a chip opens the composer prefilled as a reply through
the same SP2f reply flow a manual reply rides, and the send itself stays a
human gesture through the durable outbox, exactly as before. Chips never
auto-send, and no agent verb can reach the reply path: the CLI reads the
pool and refuses to generate (the model rule below).

## The draft pool

One row per reply generation attempted for one message
(`ai.sqlite` migration `0004`): the message's account and key, the drafted
reply and its confidence, the generation turn's correlation id, and the
store-stamped creation time. A **skipped row** (reply and confidence null)
is what a failed or unparseable generation records: the failure is
remembered, never retried by a later open, and never disturbs the reading
surface. The pool is the generation tracking: an open arms a draft only for
a message the pool holds no row for, skipped rows included.

Opening a message (OpenMessage) arms a background generation; a driver
future the host runs for the life of the app (`drive_reply_drafting`, the
incoming-pass driver's seam) takes the arming, so model latency never sits
on the open's own path. The open mints the reading turn's correlation id
and the generation's ledger rows ride it; each host pull (chips,
summarise, style) mints its own.

## Generation

One versioned prompt (`DRAFT_REPLY_PROMPT_VERSION`, draft tier), one
structured completion `{reply, confidence}`, one strict parse with a
single `repair_json` recovery, one pool row either way. The confidence is
a closed set (`low`, `medium`, `high`) stored as its fixed score (0.3,
0.6, 1.0); an unknown confidence word fails the parse whole and records a
skipped row, never a guess.

The context collector keeps the halves apart the way every Astra prompt
does: the instruction, the output policy, the style note, and the
account's knowledge context lines ride the trusted half, the note and
the knowledge lines explicitly labelled data (model-derived text over
untrusted mail must never speak as an instruction); every thread message
rides its own untrusted wrap. The thread arrives newest first from the
engine's thread read and renders oldest first with the original last, so
the model answers the message the reader has open. The caps:
`THREAD_HISTORY_LIMIT` (8) earlier members, `THREAD_BODY_CAP` (2000)
characters per message with a truncation marker, the original included,
and the knowledge context's own ten-entry cap (the assist model's read).

## The gate

A pooled draft surfaces as a chip only when its confidence clears
`CHIP_CONFIDENCE_FLOOR` (0.6, so medium and high clear it and low stays
pooled) **and** the binary judge (`JudgeDraftSurface`, nano tier) answers
Yes to the worth-surfacing question: is the drafted reply a genuinely
useful reply the user could send, not filler? The question is phrased so
Yes is the positive pole; an abstaining judge (the safe default included)
hides the chip and loses nothing: the row stays pooled and queryable
through the CLI. The **dismissal marker** (`0006`, keyed by message) is
consulted before any judge call is spent: a dismissed message never costs
another nano call. Skipped rows, blank replies, and below-floor drafts
never reach the judge at all.

`GATE_JUDGE_VERSION` versions the gate's prompt halves: the question's
phrasing and the shape of the context half. Today the context carries the
drafted reply alone, wrapped as untrusted; the original message does
**not** thread into the judge context. That boundary is the bump
condition: any change to `surface_question`'s wording, or to what the
context half carries (threading the original in, say), bumps the constant
and reruns the gate evals.

## The chips and their dismissal

The reading pane shows at most `CHIP_LIMIT` (2) chips, each a one-line
preview (`CHIP_PREVIEW_CAP` 80 characters, whitespace collapsed, `...`
when trimmed) beside the whole reply it stands for. The pane pulls chips
on open and on the `draft_pool` event the driver's sink emits when a row
lands. **Accept** dispatches the SP2f reply flow with the drafted reply as
the document's opening paragraphs (`compose_from_draft`: the reply's own
prefill, recipients, subject, and threading header, the draft's
paragraphs above the quoted original, exactly the resume path's document
convention); the composer opens as a reply and the human edits and sends
as ever. **Dismiss** records the marker and hides the message's chips for
good; the write is idempotent and the pull consults it first.

## The summarise cache

`summarise(message)` runs the Summarise use case (default tier,
`SUMMARISE_PROMPT_VERSION`) over the untrusted-wrapped message and caches
the answer in `0005`, keyed by the message identity plus the body's hash
(sixteen hex digits of the standard hasher over the plain body). A later
summarise of the same body answers from the cache with **no** network
call; a changed body hashes differently and re-summarises. The failure
direction is a false miss, never a stale hit. The cache is an
optimisation, never a gate: a summary that cannot be cached still answers.

**The key omits the prompt version.** The cache key carries the body hash,
not `SUMMARISE_PROMPT_VERSION`: bumping the summarise prompt does not
invalidate cached summaries, and old summaries keep serving until the body
changes. A version column on the row would close it if it ever bites;
recorded here as a decided nuance, not an oversight.

## The style profile

`StyleInfer` (economy tier, `STYLE_PROMPT_VERSION`) builds a per-account
style note from up to `STYLE_SAMPLE_LIMIT` (20) recent Sent messages, each
body capped at `STYLE_SAMPLE_BODY_CAP` (2000) characters, every sample
untrusted-wrapped (the user's own mail is mail all the same; the note
itself is model-derived text treated as data in the drafting prompt). The
note lives in `0004`'s sibling `style_profiles` table (account PK, note,
`built_at`) and the drafting prompt omits it entirely when none stands.

Rebuilds are demand-driven only (no background timer): a profile older
than `STYLE_PROFILE_MAX_AGE_SECS` (one day), an absent one, or a forced
rebuild infers again; a fresh profile answers from the store with no call.
No Sent mail means no profile (an empty folder never erases a built one),
and a failed or unparseable infer falls back to the stored note: a stale
note beats none. The Sent samples are drawn from a newest-window row read
(`STYLE_SENT_WINDOW`, 200 messages) filtered by Sent-role mailbox
membership, because the pinned engine rev offers no mailbox-scoped mail
read; the window is wide against the 20-sample cap and the daily cap
bounds how often the collection runs.

**The rebuild refusal (a recorded spec deviation).** The spec's D5/D6
sentence "a CLI verb `ai style rebuild` (the one mutating verb, `--preview`
describing the rebuild)" does not ship. The standing rule (the CLI never
dials a model) wins: a rebuild is a StyleInfer completion over Sent mail,
and the agent-composes parallel of `rules create` cannot apply, because an
agent-supplied note would bypass the Sent-mail grounding and could poison
the profile. The CLI ships the spelling as a refusal (exit 3) that names
the model rule and points at the app. The app-side rebuild path is the
demand-driven read above: the next drafting turn or style read after the
daily cap passes infers a fresh note. No explicit `style_rebuild` host
command ships either, because no surface calls it (A2's UI carries no
rebuild button); one would arrive with the surface that needs it, and the
CLI's pointer would name it in that change.

## The human-sends rule, restated

Nothing in this slice sends. Chips prefill a reply the human edits and
sends through the durable outbox; the summarise and style paths read and
infer, never write mail; the agent's CLI verbs read the stored tables and
refuse the one model-class spelling (`ai style rebuild`, exit 3, the
ledger row naming the refusal). Every model call, judged or completed,
lands its `llm_calls` row under the turn's correlation id, the A1
traceability contract unchanged.

## The eval pins and the live proof

The three prompts pin their constants through the same harness
(`evals/` fixtures, the seam-parameter drive): `DRAFT_REPLY_PROMPT_VERSION`
(drafted shape, the repair path, the unknown-confidence skip),
`SUMMARISE_PROMPT_VERSION` (the parsed summary, the unparseable error,
and the cached second read served from one scripted answer), and
`GATE_JUDGE_VERSION` (the useful reply surfacing, the no and
abstain-default hides, the below-floor and dismissed no-spend pins, and
the live-gated useful-reply scenario). The offline battery runs them in
CI; the live scenario rides the desktop host's live eval suite under the
`ASTRA_LIVE_LLM` gate.

The gated live suite
(`client/desktop/src/ai/live_replies_tests.rs`, under
`ASTRA_LIVE_LLM=1` plus `ASTRA_LIVE_HARNESS=1` against the running
Stalwart harness, one test at a time) proves the stages over the real
seams: a real draft for the seeded rich note (pool row, ledger cost, the
thread-history caps sanity-checked against the real thread), the chips
pull end to end over that real pool (a judge verdict spent exactly when
the confidence floor cleared), a pinned useful reply earning a real
gate verdict, the summarise answering twice on one network call (the
ledger row count is the proof), and the style note inferred live from the
Sent copy the proof's own send files, the daily cap spending no second
call and the Sent window covering the whole synced set.

## Matrix

| Rule | Status | Where |
|---|---|---|
| Draft pool with skipped-row tracking, one generation per message | ✅ | `astra-ai` migration 0004 |
| Versioned drafting prompt, closed-set confidence, one repair | ✅ | `astra-app/src/ai_replies/draft.rs` |
| Open-message arming, spawned driver, pool-presence dedup | ✅ | `astra-app/src/ai_replies/drive.rs` |
| Surfacing gate: floor, judge Yes, abstain hides, dismissal first | ✅ | `astra-app/src/ai_replies/gate.rs` |
| Chips pair, preview cap, accept into the SP2f reply flow | ✅ | `compose_from_draft`, `SmartReplies.tsx` |
| Cached summarise keyed by body hash, false-miss direction | ✅ | `astra-app/src/ai_replies/summarise.rs`, migration 0005 |
| Style profile: Sent samples, daily cap, demand-driven only | ✅ | `astra-app/src/ai_replies/style.rs` |
| Host commands: chips, summarise, style, dismiss, compose-from-draft | ✅ | `client/desktop/src/commands/ai.rs` |
| CLI reads: `ai drafts`, `ai summary`, `ai style`; rebuild refused | ✅ | `cli/src/verbs/ai.rs` |
| Eval pins for the three prompts, offline battery, live gate scenario | ✅ | `evals/`, `astra-app/src/evals/` |
| Gated live replies suite over the real seams | ✅ | `client/desktop/src/ai/live_replies_tests.rs` |
| The CLI generates drafts itself | N/A | the model rule: the app generates, the CLI reads |
| A mutating CLI rebuild verb | N/A | the recorded deviation above |

## Known gaps

- **No panic isolation around the reply-drafting driver.** The same carry
  the incoming pass records: a panic inside the driver takes the driver
  down and no later open drafts until restart. The open that armed the
  draft stays protected (the generation rides the driver's own future),
  so the loss is background drafting alone.
- **Sent samples collect before the daily cap decides.** `style_note`
  collects the Sent window and fetches up to twenty bodies before the
  profile getter's daily cap says no infer is needed, so a capped turn
  spends the collection (engine reads, no model call) for nothing. The
  check would move ahead of the collection when the collection ever costs
  enough to notice.
- **The summarise cache key omits the prompt version.** See the cache
  section; old summaries serve across a prompt bump until the body
  changes.
- **Html-only mail drafts from subject and sender alone.** The A1 carry
  (plain-text bodies only): a message whose body is html-only rides the
  drafting context with an empty body. The harness's rich note is exactly
  this shape, and the live suite drafts it anyway; a plain-text
  extraction lands with the stage that needs it.
- **Every chips pull re-judges.** The gate caches no verdict: each pull
  (the open, then the pool landing) spends a nano judge call per
  clearing row. A verdict cache keyed by pool row id would collapse
  them if the spend ever matters.
- **No explicit rebuild affordance.** The rebuild path is the
  demand-driven daily-capped read; no host command or button forces a
  rebuild (the recorded deviation above names the shape one would take).
- **The style note rides only the account that drafted it.** Profiles are
  per account and drafts read their own account's note; nothing shares a
  profile across accounts, by design.
