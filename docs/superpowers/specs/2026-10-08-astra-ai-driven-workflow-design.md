# Astra: AI-Agent-Driven Development Workflow and System Design

Date: 2026-10-08
Status: proposed (awaiting review)
Licence: the Astra repository is GPL-3.0-only (see section 2.3)

## 1. Purpose

Astra is a new desktop mail client ("Astra Mail") built by a fully AI-agent-driven
workflow covering development, testing, delivery (CI/CD), and continuous porting
from reference projects. This document is the top-level design for both the
system and the workflow that builds it.

Three rules govern every decision in this document:

1. The technology stack follows the kylins PoC (`D:\Projects\mailclient\kylins`).
2. The feature set, implementation constraints, documentation rules, and
   repository organisation follow mail-calendar
   (`D:\Projects\mailclient\opensource\mail-calendar`), porting only what
   mail-calendar has already implemented.
3. AI-related design follows inbox-zero
   (`D:\Projects\mailclient\opensource\inbox-zero`).

All documentation, code, comments, and folder names in Astra are English only.
English is the sole normative language. For review convenience, every spec,
plan, and contract document additionally carries a non-normative Chinese
review copy under `docs/zh/` mirroring the English path; wherever the two
differ, the English version governs (section 10).

## 2. Reference projects and porting strategy

### 2.1 Roles

| Reference | Role | What we take |
|---|---|---|
| kylins | Technology stack | Tauri 2 + Rust host, React 19 SPA (Vite, Tailwind v4, Zustand, React Aria), UI layout and styling, Node agent sidecar (pi SDK, JSONL over stdio, permission bridge), security implementations, CLI architecture |
| mail-calendar | Features and organisation | The full implemented feature set (see its `docs/capabilities.md`), the Intent/Surface core architecture, contract documentation rules, AGENTS.md-style rulebook, testing conventions, `xtask` gate |
| inbox-zero | AI design | LLM abstraction patterns (use-case registry, model tiers, fallback chains), structured outputs, injection hardening, DLP, per-call usage ledger, eval suites, the AI feature catalogue |
| allodia-eu/email-calendar-sync-engine | Sync engine | Consumed as a dependency, git-pinned by exact revision, the same source and pinning style mail-calendar uses |

### 2.2 Direct port strategy

Because Astra is GPL-3.0 (section 2.3), we port code directly rather than
reimplementing from documentation:

- mail-calendar's Rust core (14 crates, roughly 955 files) is copied into
  `astra-*` crates and adapted to package names and the upstream engine API.
  This is the single largest speedup: the core is client-agnostic by design.
- inbox-zero's pure TypeScript logic (prompt construction, rules matching,
  JSON repair, injection hardening) is adapted into the Node sidecar.
- inbox-zero's eval suite and LLM judge patterns are ported for Astra's AI
  behaviour gates.
- What direct porting does not cover: the React SPA is written fresh following
  kylins patterns (mail-calendar has no web client), inbox-zero's runtime glue
  (Next.js routes, Prisma, Redis) is replaced by Tauri IPC, SQLite, and local
  tasks, and the engine host layer is written against the upstream engine API
  rather than the kylins fork.

### 2.3 Licensing

- mail-calendar is GPL-3.0-only; inbox-zero is AGPL-3.0; kylins is the author's
  own work.
- Astra adopts GPL-3.0-only. AGPL-3.0 files keep their file-level obligations
  and attribution; for a local desktop application the AGPL network clause does
  not apply in practice.
- Per-file licence headers follow the REUSE convention; `xtask` checks them.
- The current Apache-2.0 `LICENSE` file is replaced with GPL-3.0 in SP0.

## 3. Product identity, versioning, and icon

- Product name: **Astra Mail**. Repository and package prefix: `astra`.
  Bundle identifier: `com.kenizhou.astra`. Name and identifier are injected at
  build time from `branding/default.env`, never hard-coded in a client
  (mail-calendar's branding rule).
- Versioning follows mail-calendar: a `VERSION` file at the repository root is
  the single source of truth and moves only in release PRs; `xtask` runs a
  version-sync check. Semver `0.x.y` during the porting phase; `1.0.0` at
  feature-complete.
- Nightly artifacts are stamped `X.Y.Z-nightly.YYYYMMDD.N` and published under
  the tag `nightly-YYYYMMDD`. Manual releases are tagged `vX.Y.Z` and kept
  forever; the keep-ten rule applies to nightlies only.
- Icon: `Images/logo.jpg` moves to `branding/` as the source image. A script
  regenerates the full Tauri icon set (`icon.ico` multi-size, `icon.png` 512,
  `icon.icns`, 32 px favicon) into `client/desktop/icons/`. Reproducible and
  replaceable.

## 4. Repository layout

mail-calendar's organisation adapted to a single desktop client on the kylins
stack:

```
astra/
  Cargo.toml            # workspace: crates/*, client/desktop, cli, xtask
  VERSION
  branding/             # default.env (name, app id) + icon source
  crates/
    astra-app/          # command surface: Intent dispatch -> Surface snapshots
    astra-viewmodel/    # pure immutable snapshots
    astra-account/      # account config -> providers, reconnect, token refresh
    astra-autodetect/   # account autodetection strategies
    astra-composer/     # ComposerDocument -> deterministic HTML
    astra-oauth/        # OAuth 2.0 PKCE client
    astra-mcp/          # local MCP agent server (opt-in, off by default)
    astra-plugin/       # plugin host: manifest, lifecycle, supervision, hook API
  client/
    desktop/            # the client: Tauri 2 Rust host (kylins backend patterns)
    web/                # React 19 SPA (kylins frontend patterns)
  sidecar/              # AI agent sidecar (Node >= 22, pi SDK, JSONL protocol)
  cli/                  # astra-cli: the AI agent's tool-call entry point
  plugins/              # first-party plugin packages (example-plugin first)
  docs/                 # contract docs + capabilities.md + changelog/
  refs/                 # refs-lock.toml: pinned baselines and porting progress
  scripts/{ci,dev}/     # harness.sh (Stalwart), changed-areas.sh, icon pipeline
  xtask/                # gate + contract checks
  .github/workflows/    # ci.yml + nightly.yml
  AGENTS.md             # the single rulebook
  LICENSE               # GPL-3.0-only
```

Deliberate deviations, agreed with the author:

- kylins' crypto workspace (SMIME/OpenPGP) is **not** ported into the core.
  Crypto returns later as first-party runtime plugins (section 6).
- The composer uses kylins' TipTap in-SPA editor rather than mail-calendar's
  standalone web-view editor; the composer-security contract still applies.
- Localisation is deferred; the UI is English-only initially. The `astra-l10n`
  crate is added when a second locale is actually needed.
- The plugin system **is** in scope (an architectural capability from kylins,
  not a mail-calendar feature); `capabilities.md` carries an Astra-specific
  plugin and hook matrix section.

## 5. Core architecture

### 5.1 Unidirectional loop (from mail-calendar)

The React SPA fires a serde-serialisable **Intent**. `astra-app` applies it
through the engine's durable outbox, recomputes the view model, and notifies
the client; the SPA then pulls an immutable **Surface** snapshot over Tauri
IPC. The surface set mirrors mail-calendar's (MailboxList, Calendar, Reading,
Sending, SyncProgress, Connectivity, Contacts, InvitationReply, DraftStatus,
and the rest). The SPA is a thin renderer; only the transport differs from
mail-calendar's UniFFI bindings.

### 5.2 Engine

The upstream `allodia-eu/email-calendar-sync-engine`, git-pinned by exact
revision in the workspace `Cargo.toml` (the same pinning style mail-calendar
uses), consumed **only** through its `engine-api` facade. The kylins fork is
not used; fork-specific adaptations are implemented in Astra's core layer
against upstream's raw shapes. Two-database model as in kylins: the engine
owns its private database; Astra owns the product database (drafts, receipts,
filters, plugin state, AI tables).

### 5.3 Host service ports (mail-calendar's ports on Tauri)

Observer -> Tauri events; account credentials -> OS keyring; OAuth browser leg
-> system browser + deep link; background sync -> tokio task in the host;
AgentHostUi -> sidecar relay.

### 5.4 Desktop shell (from kylins)

Frameless window with custom title bar; resizable three-pane layout
(FolderPane | MessageList | ReadingPane); theme system; Zustand stores
consuming surfaces; React Aria components; Tailwind v4.

### 5.5 Three front-ends, one command layer

The SPA, `astra-cli`, and the MCP server are all thin front-ends over the same
`astra-app` Intent surface. There is exactly one implementation of each
product operation.

## 6. Plugin system (runtime)

- `crates/astra-plugin/` hosts plugins: manifest declaration, lifecycle,
  process supervision (the sidecar relay and supervisor patterns reused),
  a **versioned hook API**, and a fail-closed permission model (gated tools
  default to needing approval, as the permission bridge does).
- `client/web/src/plugins/` is the JS slot-injection runtime ported from
  kylins; `plugins/` holds first-party packages, starting with the ported
  `example-plugin` as the living contract example.
- Hook surface v1, designed with crypto as the first consumer, kept minimal:
  - `compose.transform`: pre-send transform (encrypt, sign) at the
    astra-composer output seam.
  - `message.decode`: post-fetch decode (decrypt, verify) producing renderable
    parts and security banner data.
  - UI slots: reading pane banner, compose options, settings panel, account
    setup contribution points.
- Hooks never touch the engine. Send discipline (durable outbox,
  never-send-twice) cannot be bypassed by a plugin.
- `docs/plugins.md` is the contract: hook surface, manifest, permissions, API
  versioning and stability promises. Hook API changes follow the versioned
  process there; the plugin matrix lives in `capabilities.md`.
- Crypto (SMIME, OpenPGP) ships later as first-party plugin packages reusing
  the kylins crypto crates. It starts after feature parity and does not block
  the main line.

## 7. astra-cli: the AI agent's tool-call entry point

The CLI is designed **agent-first**: it is the single tool-call entry point
for Astra's built-in AI agent, present and future. Human terminal use and
scripting share the same contract as a by-product.

- Every agent tool maps to CLI verbs. kylins' six tools (safe_exec,
  session_search, generate_draft, archive_email, modify_labels, skill_view)
  and every capability added by the AI phases (rules management,
  categorisation, digest, split inbox operations, and so on) reach the data
  plane through CLI verbs only. There is no side channel.
- Permission gating always sits in front of the CLI: safeExec
  (scrubbed environment, prefix allowlist containing only `astra-cli`) maps
  dangerous verbs to virtual permission tools with previews and hard
  confirmations; send-class verbs are refused to the agent outright. Humans
  decide and humans send.
- `--json` on every verb: a stable, versioned JSON schema; errors are machine
  readable. This exists first for the agent (tool parsing), then for scripts
  and test fixtures.
- `--preview` on mutating verbs: a dry run whose output is the data source for
  permission confirmation cards. Preview and behaviour cannot drift because
  the CLI produces both.
- All CLI mutations land in the audit log, in the same stream as GUI actions.
- Shell completions are generated with clap_complete (bash, zsh, fish, pwsh).
- The verb surface grows with feature slices: each SP2+/SPA slice delivers its
  own verbs, JSON schema additions, and any new virtual permission tools in
  the same change. Cross-cutting polish is collected in an SP-CLI slice.
- `docs/cli.md` is the contract: verb table, permission gating map, JSON and
  exit-code contract. `capabilities.md` carries a CLI section.
- The MCP server faces **external** agents (opt-in, off by default, empty
  allow list, reads via stateless `query_*`, writes via Intents, no
  irreversible primitive, exactly as mail-calendar specifies). The CLI faces
  the **built-in** agent. Both dispatch `astra-app` Intents.

## 8. Security architecture

Baseline: kylins' implementations. Adjudication rule (recorded in AGENTS.md):
when implementations differ, kylins wins on security mechanics where it has
the advantage; mail-calendar wins on feature contracts.

- Process isolation and supervision (kylins): the sidecar is a separate
  process; the supervisor rejects stale generations, drains pending prompts,
  backs off linearly, and reports a Lost state.
- Fail-closed permission bridge (kylins): the policy table is pushed from the
  host; every gated tool starts as needs-approval; previews and hard
  confirmations; send-class verbs are refused to the agent.
- safe_exec (kylins): scrubbed environment, prefix allowlist, dangerous verbs
  mapped to virtual permission tools.
- Ledgers and audit (kylins): every LLM call is recorded in the `llm_calls`
  table (extended with use case, tier, and cost); audit log; circuit breaker;
  AI cache.
- Secrets (kylins): model API keys live only in sidecar memory; account
  credentials go to the OS keyring; CSP allows `https:` plus local model
  endpoints (Ollama, LM Studio).
- Privacy modes (kylins design doc): LocalOnly, Balanced, Cloud.
- Sandboxed HTML rendering and remote image gating, and certificate
  exceptions with pinning, follow mail-calendar's contracts as those features
  are ported.
- Injection hardening and DLP (inbox-zero patterns): untrusted email content
  is tagged and wrapped; DLP policies allow, redact, or block; this layers on
  top of the permission bridge.

## 9. AI architecture and feature phases

### 9.1 Process model (kylins)

A Node >= 22 sidecar spawned by the Rust host, JSONL over stdio, mirrored
protocol files (TypeScript in the sidecar, Rust in the host), supervisor with
backoff, fail-closed permission bridge, all mail mutation through `astra-cli`.
"AI advises and drafts; humans decide and send."

Interactive agent chat runs in the sidecar (pi SDK). Background and structured
tasks (rules matching, categorisation, summarisation, digest) run from the
Rust LLM client. Both sides share the tier and use-case configuration and the
usage ledger.

### 9.2 inbox-zero patterns mapped onto the stack

- Use-case registry (about 40 use cases: AssistantChat, DraftReply,
  CategorizeSender, CleanInbox, Summarise, and so on) mapped to five tiers
  (DEFAULT, ECONOMY, CHAT, DRAFT, NANO). Single source of truth in Rust
  settings, pushed to both the sidecar and the Rust LLM client.
- Fallback chains on retryable or quota errors; structured outputs with schema
  constraints and JSON repair; streaming only for user-visible text.
- Per-call usage ledger (use case, tier, cost) plus a weekly spend guard on
  the NANO tier.
- One pipeline version constant per AI feature; prompt or pipeline changes
  require evals (a rulebook rule).
- Assistant agent: tool loop with a wall-clock budget, read-before-write guards
  on durable state, pending confirmation for destructive actions (mapped to
  the permission bridge's virtual tools), progressive tool disclosure,
  context compaction, and memory extraction.

### 9.3 Phases (full inbox-zero catalogue, minus web-only surfaces)

- **A1**: AI rules (prompt-to-rules, incoming-mail rule matching via the
  background tier, deterministic action execution, learned patterns), sender
  categorisation, cold email blocker.
- **A2**: reply drafting (context collector, writing style), summarisation
  (cached), assistant chat.
- **A3**: knowledge and persona extraction, snippets, compose autocomplete,
  translate.
- **A4**: digest, meeting briefs (calendar integration), analytics reports,
  split inbox (FYI/Actioned).
- MCP in both directions: expose (ported with the mail-calendar feature set)
  and consume (the assistant reads external MCP servers for context).
- Slack/Telegram surfaces and the web onboarding assistant are out of scope
  for a desktop application; a desktop first-run assistant may be adapted in
  A4 if wanted.
- Each AI feature ships with a contract doc `docs/ai-<feature>.md` in the
  mail-calendar contract style.

## 10. Documentation and rulebook

`AGENTS.md` is adapted from mail-calendar's rulebook, in English, preserving
its discipline: 500-line file cap (enforced by xtask), newtypes for
identities, test-first for behaviour changes, bug fixes carry a regression
test at the lowest shared layer, engine only via engine-api, protocol parsing
lives in the engine, Intents stay serde-serialisable, `capabilities.md` moves
in the same change as the feature, the four-step contract change procedure
(doc rule + matrix + every shipping platform + changelog fragment), "say it
once", comments describe the present only, log lines never mention repository
paths, changelog fragments in `docs/changelog/unreleased/<slug>.md` with
Platforms and Bump tags, `VERSION` moves only in release PRs, `cargo xtask
gate` before every push, British English, and no dash used as punctuation.

Astra adds these rules: English only across documentation, code, comments,
and folder names (xtask checks for non-ASCII paths); AI behaviour changes
require evals; plugin hook API changes follow the versioned process in
`docs/plugins.md`; the security adjudication rule of section 8.

`docs/` is organised as mail-calendar organises it: a README index table of
contract docs; contract docs ported area by area as slices land (accounts,
folder-pane, search, drafts, sending, calendar, invitations, contacts, mcp,
rendering-security, composer-security, and the rest); Astra-specific docs:
`plugins.md`, `cli.md`, `upstream-sync.md`, and `ai-*.md`.
`capabilities.md` starts as mail-calendar's matrix restricted to the desktop
client, plus Astra sections for plugins, the CLI, and AI features.

Review copies: every spec, plan, and contract document (including
`capabilities.md`) carries a non-normative Chinese translation at
`docs/zh/<same-path>.zh-CN.md`, headed with a note that the English version
governs. The Chinese copy moves in the same change as its English source, so
the author can review every document in Chinese. Code, comments, identifiers,
and changelog fragments remain English only. The `xtask` gate includes a
`check-doc-sync` contract check: every in-scope English document has its
Chinese mirror, and no mirror is orphaned.

## 11. Testing strategy

- Rust core: inline sibling test files (`<module>_tests.rs`), a fakes harness,
  and gated live tests, following mail-calendar's convention.
- Test environments, three tiers:
  - **Tier 1, reproducible and secret free**: Astra builds its own Docker test
    environment the mail-calendar way: a seeded Stalwart server started by
    `scripts/dev/harness.sh`, providing JMAP and IMAP/SMTP round trips. This
    is the default live environment for developers and for CI.
  - **Tier 2, real provider accounts, environment gated, local only**: the
    author provides a local Exchange server, an outlook.com account, and a
    Gmail account. These suites follow mail-calendar's gated live sign-in
    pattern (`ASTRA_LIVE_*` environment gates). Credentials live in an
    uncommitted local environment file, never in the repository, and these
    suites never run in public CI. Requests for accounts or credentials are
    made explicitly when a slice needs them.
  - **Tier 3, AI testing**: the author provides LLM connection details on
    request. Sidecar integration tests and AI evals consume them through the
    same model configuration surface the application uses; they are stored
    locally and never committed.
- SPA: Vitest + Testing Library (kylins' setup).
- Sidecar: `node --test` plus the fixture-driven MockHarness e2e (kylins).
- Plugin host: manifest, permission, and lifecycle unit tests plus an
  example-plugin e2e.
- AI evals: scenario files and an LLM judge (inbox-zero pattern); a gate for
  AI behaviour changes.
- Gate: `cargo xtask gate`, cheapest first: formatting, clippy pedantic,
  workspace tests, contract checks (file length, version sync, dash hygiene,
  British English, licence headers, non-ASCII paths, icon sync, doc sync),
  with `--ui`, `--sidecar`, and `--plugins` extensions. CI mirrors the gate.

## 12. CI/CD

### 12.1 `ci.yml` (every push and PR)

Changed-areas gating skips unrelated jobs (ported `changed-areas.sh`). Jobs:
contract checks with formatting and clippy; workspace tests plus the Stalwart
live suite; SPA lint, test, build; sidecar test and bundle; example-plugin
build; desktop smoke builds on all three operating systems; a single required
`CI OK` check that counts skips as success (mail-calendar's pattern).

### 12.2 `nightly.yml` (every night, early morning)

Cron `30 16 * * *` UTC, which is 00:30 in Beijing, leaving two and a half
hours after the 22:00 sync task; manual dispatch is supported. Steps: check
out; stamp `X.Y.Z-nightly.YYYYMMDD.N`; build the matrix (Windows NSIS
`.exe`; macOS aarch64 and x86_64 `.dmg`; Linux `AppImage` and `.deb`);
generate release notes from commits, capability deltas, and the upstream port
summary; publish a GitHub Release tagged `nightly-YYYYMMDD`; then the
retention job deletes `nightly-*` releases beyond the ten newest.

Artifacts are unsigned initially; SmartScreen and Gatekeeper notes go in the
release body. Code signing is added later through secrets without workflow
changes when certificates exist. In the version stamp, `N` counts re-runs
within the same night and starts at 0.

### 12.3 Release flow

A release PR bumps `VERSION` and moves changelog fragments to released;
merging it, then tagging `vX.Y.Z`, triggers the release workflow. Releases are
permanent; only nightlies are pruned.

## 13. Reference management and the 22:00 sync task

### 13.1 Locks

`refs/refs-lock.toml` records, per reference repository: url, `baseline`
(the commit locked at bootstrap), `last-ported` (advancing as ports land),
and scope notes. The four repositories: mail-calendar (features and docs),
inbox-zero (AI), kylins (stack, UI, sidecar, security), and the upstream
engine (pinned revision plus the last adapted revision).

### 13.2 Lock phase

References are frozen at their bootstrap baselines until feature parity:
every mail-calendar row in `capabilities.md` is complete or explicitly N/A,
AI phases A1 to A4 are done, and the plugin host is in place.

### 13.3 The 22:00 task (ZCode scheduled automation, local)

Run nightly at 22:00 local time, idempotent, anchored on `last-ported`
commits so missed nights are caught up on the next run:

1. Fetch all four references; diff `last-ported..HEAD`; skip if empty.
2. Classify changes: docs only, tests only, refactor, behaviour, new feature,
   security.
3. Port per the rulebook (contract doc, matrix, changelog fragment, and the
   Chinese review copy of the doc land in the same change). Security-tagged
   changes are always held for review and never auto-merged.
4. Run the fast local gate subset (the full gate runs in CI).
5. Open a PR with a per-repository summary. Auto-merge only for low-risk
   classes with a green gate; anything else waits for morning review with a
   digest.
6. On merge, advance `last-ported`.
7. UI style decisions and architectural conflicts are listed explicitly in
   the PR for the author to decide; silent deviation is forbidden.
8. Engine updates arrive by bumping the pinned revision, adapting, and
   following the same PR flow.

The 00:30 nightly build publishes whatever is on `main`: low-risk auto-merged
ports ship the same night; held ports ship after morning review.

## 14. Daytime development workflow (AI-agent-driven)

- The slice backlog is the set of open rows in `capabilities.md`, dependency
  ordered, plus the AI phases.
- Slice loop: a short spec when non-trivial; a plan; parallel subagent
  implementation (independent slices in parallel, shared crates serialised);
  test-first for behaviour changes; `xtask gate`; a PR with conventional
  commits, a changelog fragment, and the capabilities update; a code-review
  agent pass; the author merges.
- UI decision gate: wherever kylins' UI seems unreasonable and mail-calendar
  or inbox-zero looks better, work stops and the question is discussed with
  the author before implementation. The same gate binds the 22:00 task.
- Daily rhythm: slice development by day; upstream porting at 22:00; the
  nightly build at 00:30; PR review in the morning.

## 15. Sub-project decomposition

Each sub-project runs its own spec -> plan -> implement cycle.

- **SP0 Bootstrap**: pull and lock all four references; full repository
  skeleton per section 4; LICENSE to GPL-3.0; AGENTS.md rulebook; xtask gate
  skeleton; icon pipeline from `logo.jpg`; CI skeleton; nightly skeleton;
  VERSION and branding; docs skeleton with the seeded capabilities matrix.
- **SP1 Core architecture**: crate skeletons; engine integration (upstream
  pin; engine host written against the upstream API); Intent/Surface
  protocol v1; Tauri host and React shell with the kylins layout; plugin
  host skeleton; the first vertical slice (add an IMAP account, sync, see
  folders) including its minimal CLI verbs.
- **SP2+ Feature slices**: accounts, folder pane, message list, reading,
  compose, drafts, outbox and sending, search, threading, calendar,
  invitations, contacts, settings, MCP server; each slice carries its CLI
  verbs and contract doc.
- **SPA0 AI foundation**: sidecar port; tier and use-case registry; usage
  ledger; injection hardening.
- **SPA1 to SPA4**: the AI phases of section 9.3.
- **SP-CLI**: cross-cutting CLI polish (completions, help quality,
  performance) once the verb surface is broad.
- **SP-CRYPTO** (after parity): plugin API polish; SMIME and OpenPGP
  first-party plugins.
- The 22:00 sync automation is built and rehearsed during SP0/SP1 (dry runs
  against locked references) and activated at parity.

## 16. Open items

- Desktop UI automation approach: kylins' MockHarness-driven e2e versus
  WebDriver-driven webview tests; decided during SP1.
- Provider reachability of the tier 2 accounts (outlook.com personal accounts
  and on-premises Exchange against the upstream engine's provider set) is
  resolved with live tests during the SP2 accounts slices.
- A desktop first-run assistant (adapted from inbox-zero's onboarding) is
  optional in A4; decide then.
- Code signing certificates for Windows and macOS; add when available.
- macOS notarisation and the Tauri updater channel are out of scope until
  signing exists.

## 17. Decision log

| Decision | Choice | Alternatives rejected |
|---|---|---|
| Client architecture | Single Tauri client over the mail-calendar core discipline | UniFFI multi-client preparation (YAGNI); kylins package layout without the Intent/Surface protocol |
| Platforms | Windows, macOS, Linux | Windows only |
| Artifact hosting | GitHub Releases with ten-nightly retention | Self-hosted server upload; both |
| Engine | Upstream, pinned by revision | The kylins fork (author's call: same source as mail-calendar) |
| Licence | GPL-3.0-only, direct port | Apache-2.0 with clean-room reimplementation (much slower); private-only |
| Plugins | Runtime: out-of-process backend hooks plus JS slot-injection frontend | Compile-time trait crates; frontend-only with reserved seams |
| AI scope | Full inbox-zero catalogue, phased A1 to A4 | Core set only; core plus personalisation |
| Orchestration | Hybrid: cloud CI and builds, local ZCode automation for porting | All-cloud agent in CI (key management, cost); all-local (no macOS builds possible) |
| CLI positioning | Agent-first tool-call entry point; human CLI shares the contract | Human-first CLI with an agent adapter layer |
| Security baseline | kylins implementations win on mechanics; mail-calendar wins on feature contracts | Per-case ad hoc decisions |
| Documentation language | English normative; non-normative Chinese review copies under `docs/zh/`, moved in the same change | English only (hard for the author to review); bilingual normative (drift risk) |
