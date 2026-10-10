# AI foundation: the Astra Mail contract

**Scope.** The SPA0 AI skeleton: the tier and use-case registry, the
background LLM client, the `ai.sqlite` ledgers, the correlation-id
traceability chain, and the sidecar loop contract they serve. Binding on
every AI feature that ships later (A1+); the phases themselves live in
the capabilities matrix and the parent spec's section 9.

**Principle.** *AI advises and drafts; humans decide and send.* Every
model call is reconstructible from the ledgers, every mail mutation the
agent caused is traceable to the prompt and the approval that produced
it, and no key or credential travels anywhere it could leak.

## The three principles

The sidecar and everything around it are **light**, **secure**, and
**fully traceable** (parent spec section 9.1):

- **Light.** One esbuild-bundled CommonJS artifact with pinned, minimal
  dependencies; no direct database or engine access, the data plane is
  reached only through `astra-cli`; no state beyond session transcripts;
  cheap to spawn, supervise, and replace.
- **Secure.** Model API keys live only in memory; the spawn environment
  is an allowlist; every child the sidecar starts runs through
  `safe_exec` with a scrubbed environment and an argv prefix check; the
  permission bridge fails closed; send-class verbs are refused
  structurally, so sending stays a human action.
- **Traceable.** Every LLM call lands in the `llm_calls` ledger; every
  tool call and permission decision lands in the audit log with verb,
  argument preview, outcome, and correlation id; and one correlation id
  per agent turn runs from the UI through the host, the sidecar, the
  provider call, and the CLI into every row, so any observed mail change
  traces back to the prompt and the approval that caused it.

## The registry

Six use cases ship in SPA0, each mapped to one of five tiers
(ported from inbox-zero's use-case table):

| Use case | Tier |
|---|---|
| `assistant_chat` | chat |
| `draft_reply` | draft |
| `summarise` | default |
| `categorize_sender` | default |
| `clean_inbox` | default |
| `agent_tool` | economy |

Tiers are configured by environment variable, one comma list per tier:
`ASTRA_LLM_DEFAULT`, `ASTRA_LLM_ECONOMY`, `ASTRA_LLM_CHAT`,
`ASTRA_LLM_DRAFT`, `ASTRA_LLM_NANO`. Each entry is `provider:model`
(for example `openai:gpt-4o-mini,anthropic:claude-3-5-haiku-latest`);
the first entry is primary and the rest are the fallback chain. The
provider prefix (`openai` or `anthropic`) selects the wire protocol.
A blank value reads as unconfigured; a malformed list is a loud error
naming the variable. When a tier is unconfigured, economy, chat, and
draft fall back to the default tier's chain and nano falls back to
economy then default.

Provider credentials and endpoints are environment variables too, read
once at boot and held in memory:

- Keys: `ASTRA_LLM_API_KEY_OPENAI`, `ASTRA_LLM_API_KEY_ANTHROPIC`
  (the pattern is `ASTRA_LLM_API_KEY_<PROVIDER>`, provider uppercased).
- Base-url overrides: `ASTRA_LLM_BASE_URL_OPENAI`,
  `ASTRA_LLM_BASE_URL_ANTHROPIC`. The defaults are
  `https://api.openai.com/v1` and `https://api.anthropic.com`; the
  override exists so a compatible endpoint (a fixture, or a
  DeepSeek-style anthropic-compatible host) can be named. A trailing
  `/v1` pasted onto the anthropic base is forgiven.

**Tier-in-rows semantics.** Tier-level fallback collapses before the
client sees a chain (`TierTable::resolve`), so every ledger row of one
call carries the use case's own tier; entry-level advancement within the
chain is visible as differing `model` values across that call's rows.

## The ledgers and the traceability contract

With SPA0 the AI ledger arrives as product data: `ai.sqlite` is created
beside `astra.sqlite` in the application data directory, opened by the
host at boot and by every `astra-cli` invocation, one ledger for both
writers (a five second busy timeout covers their overlap).

Two tables:

- `llm_calls`: id, created_at, correlation_id, use_case, tier, model,
  prompt_tokens, completion_tokens, cost_micros (nullable), latency_ms,
  outcome.
- `audit_log`: id, created_at, correlation_id, verb, arg_preview,
  outcome.

Who writes:

- The background client writes one `llm_calls` row per HTTP attempt
  (outcome words: `ok`, `error:transport`, `error:auth`,
  `error:quota`, `error:rate_limited`, `error:http_<code>`,
  `error:bad_response`).
- The host relay writes a `llm_calls` row for each sidecar-reported
  `llm_call` envelope, an `audit_log` row for each audited tool outcome
  (verb = the tool name), and one for each permission decision
  (verb = `permission <tool>`, outcome `approved`, `rejected`, or
  `fail-closed`).
- The CLI writes an `audit_log` row for every invocation, refused and
  preview lines included, with a redacted argument preview (shapes and
  ids only, never addresses, subjects, or body text).

The correlation id is a UUID minted per agent turn (or supplied by the
caller) and is the contract that binds it all: the prompt carries it to
the sidecar; every tool spawn of the turn carries it as the CLI's
`--correlation-id` flag and as the `ASTRA_CORRELATION_ID` environment
variable of the child; the CLI stamps it on its audit rows; the
sidecar's `llm_call` envelopes carry it back to the host's ledger; and
every permission ask and decision of the turn is audited under it. The
offline e2e (`client/desktop/src/agent/e2e_tests.rs`) proves the chain
against the real bundle: the permission row, the tool row, and the
`llm_call` row of one scripted turn share one id, and so does the argv
and environment the CLI child actually received.

## The store: rusqlite, not sqlx (recorded decision)

The plan named sqlx. That is impossible in this workspace, structurally:
sqlx 0.9 requires `libsqlite3-sys >= 0.30.1, < 0.38`, while the mail
engine pin pulls `rusqlite 0.40`, which needs `libsqlite3-sys 0.38.x`;
Cargo allows exactly one crate with `links = "sqlite3"` per resolve, so
the workspace does not even resolve with sqlx present. The decision,
upheld through review: `AiStore` rides rusqlite 0.40 (bundled build),
the engine's own sqlite stack, so one `libsqlite3-sys` links and the
engine store and the AI store share one sqlite. Migrations are
hand-rolled, one transaction per migration with its bookkeeping row in
an `ai_migrations` table, and the file naming stays sqlx-shaped
(`0001_init.sql`) should the store migrate engines later. The API is
synchronous (a mutexed connection); there is no `query!` macro and no
build-time `DATABASE_URL` by construction.

## The background client and the sidecar loop

The background client (`client/desktop/src/ai/`) serves non-chat
completions: dual protocol (openai `POST {base}/chat/completions` with
a bearer key; anthropic `POST {base}/v1/messages` with `x-api-key` and
`anthropic-version: 2023-06-01`, `max_tokens` 1024), one attempt per
chain entry, retryable failures (connect and transport errors, 5xx,
429, malformed 2xx envelopes) advancing to the next entry, `Retry-After`
honoured in delta-seconds capped at two seconds, and auth (401, 403) and
quota (402, the providers' exhausted-balance bodies) terminal for the
call. Error strings are status-level: response bodies never flow into
them. No feature consumes the client until A2; the chat panel's model
rides the sidecar.

The sidecar loop is the panel's engine: the host spawns
`node <sidecar>/index.cjs` (Node 22 or newer), the init handshake
delivers the cli path, the data dir, the permission policy, and the
model config with its key **in band** (the spawn environment is an
allowlist of `PATH` and `ASTRA_DATA_DIR` plus Node's Windows platform
minimums, nothing else, so no `ASTRA_LLM_*` variable can ride in), the
turn's events stream back as JSONL, gated tools suspend on approval
cards the user answers, and every tool call funnels through `safe_exec`
into `astra-cli`. Reads run automatically; `save_draft` and
`discard_draft` ask; everything unknown fails closed.

## Manual LLM smoke checklist

For the author, against a live endpoint (connection info: an OpenAI or
Anthropic key, and the model names on that account; a compatible base
url when the endpoint is not the provider's own). Every command runs
from the repo root in Git Bash. No automated network test exists by
design; this checklist is the live proof.

1. **Prepare once.**

   ```
   npm --prefix sidecar ci
   npm --prefix sidecar run build
   cargo build --release --bin astra-cli
   npm --prefix sidecar run smoke
   ```

   The last command drives the built bundle through the real pi path to
   an unreachable endpoint and must report the failure cleanly.

2. **Live chat through the app.** Export the connection, then run the
   desktop host:

   ```
   export ASTRA_LLM_CHAT=openai:gpt-4o-mini
   export ASTRA_LLM_API_KEY_OPENAI=sk-<your key>
   # optional, for a compatible endpoint:
   export ASTRA_LLM_BASE_URL_OPENAI=https://<host>/v1
   cargo run -p astra-desktop
   ```

   (For Anthropic: `ASTRA_LLM_CHAT=anthropic:<model>` and
   `ASTRA_LLM_API_KEY_ANTHROPIC=sk-ant-<your key>`, with the optional
   `ASTRA_LLM_BASE_URL_ANTHROPIC=https://<host>`.)

   Open the Agent panel from the status bar, send `Say hello in one
   sentence`, and watch the reply stream. Then send `Save a draft to
   bob@example.com about Friday` and approve the card; the panel shows
   the tool result.

3. **Verify the ledgers.** One correlation id must span the turn's rows
   (Windows path shown; macOS
   `~/Library/Application Support/com.kenizhou.astra`, Linux
   `~/.local/share/com.kenizhou.astra`):

   ```
   sqlite3 "$APPDATA/com.kenizhou.astra/ai.sqlite" \
     'select correlation_id, use_case, tier, model, outcome from llm_calls order by id desc limit 5;'
   sqlite3 "$APPDATA/com.kenizhou.astra/ai.sqlite" \
     'select correlation_id, verb, outcome from audit_log order by id desc limit 8;'
   ```

4. **Sidecar alone (no app).** Drive the bundle by hand through the
   real pi harness (one shell line per envelope; the `sleep` holds
   stdin open while the turn streams). The key is read from a file, so
   the line the shell history keeps never contains it:

   ```
   key=$(cat /path/to/keyfile)
   ( printf '%s\n' \
     '{"type":"init","harness":"pi","cli_path":"<absolute path>/target/release/astra-cli","data_dir":"<absolute data dir>","model_configs":{"chat":{"api":"openai-completions","base_url":"https://api.openai.com/v1","model":"gpt-4o-mini","api_key":"'"$key"'"}},"policy":{"list_folders":{"level":"auto"},"list_messages":{"level":"auto"},"read_message":{"level":"auto"},"save_draft":{"level":"confirm"},"discard_draft":{"level":"confirm"}}}' \
     '{"type":"prompt","id":"p-1","session_id":"agent:main","text":"Say hello in one sentence.","correlation_id":"corr-smoke"}' \
     ; sleep 20 ) | node sidecar/dist/index.cjs
   ```

   Expect `ready`, streamed `text_delta` events, an `llm_call` envelope
   carrying `corr-smoke`, and `done`.

5. **Clean up.** `unset ASTRA_LLM_CHAT ASTRA_LLM_API_KEY_OPENAI
   ASTRA_LLM_BASE_URL_OPENAI key` (and the anthropic spellings when
   used).

**Where the key travels.** From the exported environment into the host
process at boot, once; then in band on the init envelope over stdio into
the sidecar's in-memory model registration; nowhere else. It never
enters the sidecar's spawn environment (the allowlist above), never
enters any child the sidecar spawns (`safe_exec` scrubs to `PATH`, the
`ASTRA_*` passthrough, and the injected data dir and correlation id),
never lands in a ledger row or log line, and error messages are
status-level, so response bodies cannot echo it back.

## The eval harness

The rulebook requires an eval run for every AI behaviour change. The
harness ships with A1: scenario fixtures live under `evals/` (one JSON
file per scenario: its feature, the pipeline constant value its prompt
text was written against, its inputs, and its expected outcome) and the
runner in `astra-app` drives them through the same seams the unit tests
exercise. Deterministic scenarios run as ordinary offline tests in CI;
judge-backed scenarios run only under the `ASTRA_LIVE_LLM=1` gate.
Every scenario pins the pipeline constant it was written against, so
bumping a prompt constant without rerunning and refreshing its
scenarios fails the offline suite.

## Known gaps

- **Single model registration.** The pi harness registers exactly the
  first delivered tier's model; one chat model, no per-session switch.
- **No streaming client.** The background client is completion-only;
  streaming arrives with A2's chat surfaces (the sidecar's own text
  streams today).
- **DLP patterns are heuristic.** `ASTRA_DLP` selects the posture
  (`allow`, the default, `redact`, or `block`) and the `apply` gate
  checks every mail-carrying prompt; the masked shapes (addresses,
  phone runs, long digit runs) are pinned by the offline tests beside
  the eval fixtures under `evals/`, which also document the corners the
  heuristics leave unmasked.
- **The CLI second-writer seam.** `astra-cli` opens the same
  `astra.sqlite` as the running app (WAL, busy timeout, the engine's
  outbox lease keep the writers safe), but the app is not pushed: it
  learns a CLI draft on its next sync or refresh.
- **npm audit at the pi pin.** 1 moderate and 2 high advisories in the
  transitive tree of `@earendil-works/pi-coding-agent` 0.84.2 (the
  kylins parity pin). The sidecar is host-spawned and talks only to the
  configured model endpoint.
- **Duplicate drafts until `--replacing`.** `drafts save` takes no
  replace flag, so a second save of one composition stores a second
  draft beside the first.
- **Malformed init drops silently.** A present-but-malformed init line
  is skipped with a stderr warning; the host observes a ready timeout
  rather than a typed error.
- **`discard_draft` gates at confirm.** Deleting a draft asks once at
  the confirm level; kylins' harder confirm was dropped because Astra
  ships no delete-class verb beyond drafts.
- **Single-session correlation attribution.** The relay's single global
  correlation holder and the `any_correlation` attribution of
  session-less asks and tool results are single-session constraints;
  they gain per-prompt stamping when A2 brings concurrent chat
  surfaces.
