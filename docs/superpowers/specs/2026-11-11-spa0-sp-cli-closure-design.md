# SPA0 + SP-CLI design: closing the two placeholder rows

**Status.** Decided 2026-11-11. Slices branch from main at this commit.

**Scope.** The matrix's last two unstarted squares, both in the CLI
section: `Virtual permission tools mapping` (SPA0, capabilities.md:152)
and `Shell completions` (SP-CLI, capabilities.md:153). The first makes
the sidecar's implicit tool-to-permission mapping an explicit, exhaustive,
test-pinned contract artifact; the second ships a `completions <shell>`
verb generated from the CLI's own vocabulary. Neither changes the
model-facing tool surface, so neither triggers the AI eval rule (the
judgement is recorded under D1).

## D1: the permission gating map (SPA0)

A "virtual permission tool" is a gated tool-surface entry whose execution
suspends behind a human approval card; the founding spec
(2026-10-08-astra-ai-driven-workflow-design.md:211,226) names the mapping
and requires `docs/cli.md` to carry it as a "permission gating map". Today
the mapping is implicit and scattered: tool name to CLI verb in the
sidecar's `mailTools.ts` execute bodies, tool name to policy level in the
host's `default_policy()` (agent/mod.rs:329), and tool name to redacted
preview shape in `shaped_arg_preview` (agent/permissions.rs:207). The
bridge already fails closed on any tool absent from the policy table; what
is missing is the explicit contract and the pins that keep the three
surfaces answering for every tool.

The slice ships:

- **The map itself.** A new "The permission gating map" section in
  `docs/cli.md`: one exhaustive row per sidecar tool naming its CLI verb
  and subverb, its policy level, and its redacted preview shape. The ten
  tools: four reads run (`list_folders`, `list_messages`, `read_message`,
  `search_messages` at `auto`), six writes ask (`save_draft`,
  `discard_draft`, `mark_read`, `set_flagged`, `archive_message`,
  `move_to_trash` at `confirm`). The section states the standing rule: a
  tool absent from the map fails closed in the bridge, and adding a tool
  forces the map, the policy table, and the preview vocabulary to answer
  for it (the exhaustive-match precedent in docs/plugins.md:66-81).
- **The host pins.** A test pinning `default_policy()` to exactly the ten
  tools at the documented levels. `shaped_arg_preview` grows explicit arms
  for the five tools that today fall to the generic `fields=N` default
  (`search_messages`, `mark_read`, `set_flagged`, `archive_message`,
  `move_to_trash`); the generic fallback stays for a genuinely unknown
  tool (fail closed). A test pins all ten shapes.
- **The sidecar pin.** A test pinning that `createMailTools` yields exactly
  the ten names and that each tool's `buildCliArgv` verb and subverb match
  the map.

**The no-eval judgement, recorded.** The five new preview arms change the
host's approval-card preview string and the permission audit row's
preview, both host-side. The model-facing tool surface (the ten tool
definitions, their parameters, and their argv) is unchanged, and no policy
level moves. The AI rule binds a "prompt, pipeline, tool surface" change;
this is none of those, so no eval run and no pipeline bump ride this slice.
The sidecar and CLI pins add tests, not behaviour.

**The row moves to shipped.** The mapping (tool to verb to level to
preview shape) is complete, explicit, and pinned on every surface, so
capabilities.md:152 moves ⬜ to ✅ with the Notes naming cli.md's gating
map. The preview-PRODUCTION leg is a separate concern, recorded as a known
gap below, not a shortfall of the mapping itself.

## D2: shell completions (SP-CLI)

A new `completions <shell>` verb generates a completion script for bash,
zsh, fish, and pwsh from the CLI's own vocabulary: the verb words from
`Verb::name()`, the subverb words from `SUBVERBS` (parse.rs:398), and a
per-verb flag descriptor. The verb answers without the engine (the store
verbs' discipline), is non-mutating (`mutating()` false, so the shared gate
refuses `--preview`), and is audited like every invocation.

**Two deviations, recorded rather than silent (rulebook, upstream sync).**

- The founding spec (2026-10-08:221) says completions are "generated with
  clap_complete". The shipped parser is hand-rolled and clap-free;
  introducing clap would rewrite the parser. This slice generates from the
  existing vocabulary instead, keeping the one-source-of-truth discipline
  the codebase pins by tests. The deviation is recorded here, in cli.md,
  and in the PR.
- The envelope rule (cli.md:153) says every invocation prints one JSON
  envelope. Completions is a human-facing generator the sidecar never
  calls, so it prints the raw script by default (the `eval "$(astra-cli
  completions bash)"` idiom) and wraps it in the standard envelope under
  `--json` (`data` carries `shell` and `script`, schema still 1). This is
  the one generator exception to the envelope rule; cli.md's envelope
  section is amended to state it. The agent-facing uniform envelope is
  intact because the sidecar never invokes completions.

**The flag vocabulary is spelled twice, by design.** The verb and subverb
words have single sources (`Verb::name()`, `SUBVERBS`); the per-verb flags
live inline in the parse match arms, so the completion descriptor spells
them a second time and a drift test pins the two together, the
`CLOSED_ACTIONS` precedent (cli.md:145-149). An exhaustive match over
`Verb` forces a descriptor entry for every verb, so a new verb cannot ship
without its completion vocabulary.

**The row moves to shipped.** capabilities.md:153 moves ⬜ to ✅; cli.md's
verb table gains the completions row (the count moves thirty-one to
thirty-two), the matrix gains a completions row, and the envelope section
notes the generator exception.

## Known gaps recorded, not hidden

- **The preview-production leg (SPA0).** The bridge and the wire protocol
  carry `confirm_preview` and an approval `preview` payload end to end
  (protocol.ts:89-97, protocol.rs:56, permissions.rs:31), but nothing
  produces them: `runGated` passes no preview to `requestApproval`, the
  sidecar never spells `--preview`, `default_policy()` uses no
  `ConfirmPreview`, and the approval card renders the redacted args, not
  the action preview. Wiring `--preview` output onto the card is a deeper
  slice (the confirm_preview cluster); this slice documents the redacted
  preview shape the map carries and records the production leg as the gap.
- **SP-CLI's wider scope.** The founding spec's SP-CLI line names
  "completions, help quality, performance" (2026-10-08:505). This slice
  closes the "Shell completions" row only; help quality and performance
  stay for a later SP-CLI slice.
- **The flag descriptor's duplication cost.** The per-verb flags are
  spelled in parse and in the descriptor, pinned by a drift test; the cost
  is the closed action set's own, recorded in docs/ai-rules.md.

## Test-first

Every behaviour lands with its red. SPA0: the policy-table pin (exactly
ten tools, the documented levels), the ten preview shapes (the five new
arms and the five existing), the sidecar's ten-name and argv pin; the
offline agent e2e and the policy tests stay green (the e2e exercises
`save_draft`, whose shape does not move). SP-CLI: each of the four shells
produces a non-empty script naming every verb; an unknown shell is a usage
error (exit 2); `--json` answers the schema-1 envelope with `data.shell`
and `data.script`; `--preview` is the shared non-mutating usage error; the
raw form prints the script, not an envelope; the descriptor covers every
verb and matches parse for the representative verbs; the audit row lands
with the turn's correlation id. No live model proof rides this slice (no
model-facing behaviour moves); the offline battery is the proof.