# AGENTS.md

The single rulebook for Astra, written for humans and coding agents alike.
Every rule lives here or in the one document it names; everywhere else links.

## Product

Astra Mail is a desktop mail client. The product is named "Astra Mail",
never "Astra" alone in user-facing copy. Name and app id come from
branding/default.env at build time.

## Writing

- Brief and concrete. Say it once: a rule lives in exactly one place.
- British English (`colour`, `licence` noun, `license` verb). `dialog`,
  `artifact`, and `catalog` stay American.
- No dash as punctuation: no em or en dash anywhere, and no spaced hyphen
  as punctuation in Markdown.
- Code comments describe the present only; no history, no plan phases,
  sparingly. Log lines never mention repository paths.
- English only across documentation, code, comments, and folder names.
  `docs/zh/**` holds non-normative Chinese review copies; when a document
  changes, its review copy changes in the same commit.

## Hard rules

- Source files stay under 500 lines. The gate enforces it for every source
  language; Markdown prose is exempt.
- Newtypes for identities; no raw `String` ids across module boundaries.
- Test-first for behaviour changes. A bug fix lands with a regression test
  at the lowest shared layer that observes the contract.
- The engine is consumed only through `engine-api`. Protocol parsing belongs
  in the engine, never in Astra.
- Intents stay serde-serialisable. Surfaces are immutable snapshots.
- `docs/capabilities.md` moves in the same change as the feature it
  describes.
- `VERSION` moves only in a release PR and always together with the
  workspace package version.

## Contract changes

A contract is decided once in its document under `docs/` and binds the whole
product. Changing a contract takes: (1) the doc rule and matrix update,
(2) the implementation on every surface that ships it, (3) the
`docs/capabilities.md` row, (4) a changelog fragment in
`docs/changelog/unreleased/<slug>.md` with `Platforms:` and `Bump:` tags.
Shortfalls are recorded as known gaps, never left silent.

## Changelog fragments

One file per user-visible change: `docs/changelog/unreleased/<slug>.md`
containing the note, then `Platforms: desktop` and `Bump: none|minor|major`.
Fragments move to `docs/changelog/released/` in the release PR. Fragments
are English only and have no Chinese mirror.

## Security

kylins implementations win on security mechanics where they have the
advantage; mail-calendar wins on feature contracts. The sidecar is fail
closed: every gated tool starts as needs-approval, send-class verbs are
refused to the agent, and humans decide and send. Every LLM call, tool
call, and permission decision lands in its ledger with a correlation id.

## AI rules

- An AI behaviour change (prompt, pipeline, tool surface) requires an eval
  run in the same PR and a pipeline version bump.
- Model API keys live only in sidecar memory. Background tiers run from the
  host with the shared tier and use-case registry.

## CLI rules

`astra-cli` is the agent's tool-call entry point, designed agent-first.
Every verb ships with `--json` (versioned schema) and mutating verbs ship
with `--preview`. New verbs land in the same change as the feature they
expose, together with any new virtual permission tool.

## Plugin rules

Plugins run out of process against the versioned hook API in
`docs/plugins.md`. Hooks never touch the engine; send discipline cannot be
bypassed. Hook API changes follow the versioned process in that document.

## Upstream sync

References are locked per `refs/refs-lock.toml`; the process and the 22:00
task contract live in `docs/upstream-sync.md`. Silent deviation from a
reference decision is forbidden: disagreements are escalated in the PR.

## Build and verify

- Git Bash on Windows is the shell; bash 5 or newer.
- `cargo run -p xtask -- gate` before every push. CI mirrors the gate.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` are the floor.
- Traps discovered while working go into this section, appended one line
  each.
- rust-toolchain.toml overrides the CI toolchain: the pinned channel must
  list the components it needs, or fmt and clippy go missing on the runner.
