# CLI: the Astra Mail contract

**Scope.** The `astra-cli` binary: the agent-first tool-call entry point the
AI sidecar executes, its verbs, the `--json` envelope, `--preview` on the
mutating verbs, the per-invocation audit, and the exit codes a caller
dispatches on. Binding on every surface that drives the CLI (today the
sidecar's `safeExec`; tomorrow anything else that shells out to it). The
rulebook's CLI rules state the standing policy; this document is the
contract the shipped verbs answer to.

**Principle.** *The CLI is what an agent is allowed to do with mail, no
more.* Every verb reads or drafts; sending stays a human action; and every
invocation, refused ones included, leaves a row in the audit ledger tying it
to its turn's correlation id. An unaudited invocation is not answered at
all: the process fails closed rather than print an envelope for something
the ledger does not know happened.

## The verbs

Thirty-two ship today: fourteen mail, drafts, and rules verbs, the ten `ai`
verbs, the seven `knowledge` and `snippets` verbs, and the `completions`
generator. The grammar is two positional words (`verb subverb`) plus
flags, and flag order is free: the sidecar
should spell `astra-cli [--json] [--correlation-id <id>] <verb> <subverb>
[--flag <value> ...]`, always passing `--json` and the turn's
`--correlation-id`, and treating everything after the two positionals as
named flags. The `ai` store verbs take their remaining operands as
bare positionals instead, the grammar their surfaces pinned; the
`knowledge` and `snippets` verbs read and write their stores through
named flags like the mail verbs. The `completions` verb rides its shell
word in the second positional slot (`completions <shell>`), one of bash,
zsh, fish, or pwsh. A call is unambiguous in text and in an audit row.

| Verb | Reads | Answer's `data` |
|---|---|---|
| `accounts list` | the account index (`accounts.json`) in the data dir | `accounts`: the ids, in index order |
| `folders list --account <id>` | the engine store's mailbox objects | `folders`: id, name, role, parent, `unread_count`, in the sidebar's canonical order, the tree flattened with each row carrying its parent so nesting is derivable |
| `messages list --account <id> [--folder <key>] [--limit <n>]` | the engine store's windowed list read | `messages`: key, from, subject, date, unread, draft, has attachment, preview |
| `messages read --account <id> --key <k> [--key <k>...]` | the engine store's whole-message read and cached body | `messages`: the envelope plus `body.plain` and `body.html_present` |
| `messages search --query <q> [--account <id>] [--folder <key>] [--limit <n>] [--offset <n>]` | the engine's search surface, paged | `rows` (account, key, subject, sender, date, unread, flagged, has attachment; no bodies), `total`, `offset`, `has_more`, `windowed`. Without `--account` the search covers every account in the index except each account's own Trash; an account covers its whole mailbox; a folder narrows within the named account and requires it. Pages hold twenty by default, fifty at most |
| `messages mark-read --account <id> --key <k> [--unread]` | writes the `$seen` keyword edit through the engine's outbox | `key`, `op`, `state` (`applied` or `queued`); `--preview` prints the exact edit |
| `messages set-flagged --account <id> --key <k> [--clear]` | writes the `$flagged` keyword edit through the engine's outbox | `key`, `op`, `state`; `--preview` prints the exact edit |
| `messages archive --account <id> --key <k>` | moves the message to the account's archive through the engine's outbox | `key`, `op`, `state`; `--preview` prints the exact move |
| `messages trash --account <id> --key <k>` | moves the message to the account's trash through the engine's outbox; recoverable from there | `key`, `op`, `state`; `--preview` prints the exact move |
| `drafts save --account <id> --from <a> --to <a,...> [--cc <a,...>] --subject <s> (--plain <text> or --file <path> or stdin)` | writes through the engine's `put_draft` | `message_id`, `op`, `state` (`stored` or `queued`), and `key` when stored |
| `drafts discard --account <id> --key <k>` | writes through the engine's `delete_draft` | `key`, `op`, `state` (`removed` or `queued`) |
| `rules list` | the rules store beside the audit ledger | `rules`: id, name, condition, `actions` as wire words, enabled, `created_at`; a stored payload outside the closed set fails the verb loudly, naming the rule |
| `rules create --name <n> --condition <c> --action <word> [--action <word>...]` | writes one rule to the rules store, enabled | `rule` (the whole stored rule) and `state: created`; `--preview` prints the exact rule the store would take |
| `rules delete --id <id>` | deletes one stored rule | `id`, `state: deleted`; `--preview` names the exact deletion; an id no rule carries is a runtime error naming it |
| `ai cold <account> <key> [<key>...]` | the stored cold verdicts in the results store | `messages`: per key, the newest `cold` row's `verdict` with the `correlation` that judged it and `recorded_at`, or `null` when no pass has judged the message |
| `ai results <account> <key> [<key>...]` | the stored stage rows in the results store | `messages`: per key, every row newest first with `stage`, `rule_id`, `verdict`, `category`, `correlation`, `recorded_at` |
| `ai drafts <account> <key> [<key>...]` | the stored draft pool rows (migration 0004) beside the audit ledger | `messages`: per key, every pool row newest first with `correlation`, the whole `reply` (model-generated text the verb exists to read, the same whole-body discipline `messages read` carries), `confidence`, `skipped`, `created_at`; a message no generation touched answers an empty list |
| `ai summary <account> <key> [<key>...]` | the stored summarise cache row (migration 0005) beside the audit ledger | `messages`: per key, the cached `summary` with the `body_hash` it was cached under and `created_at`, or `null` when nothing is cached; whether the row still hits is the app's serving-time body-hash check, which this read makes no claim about |
| `ai style <account>` | the stored style profile (0004's sibling table) | `style`: the profile's `note` and `built_at`, or `null` when none was ever built |
| `ai style rebuild` | nothing: the spelling parses and refuses | exit `3` with the model rule's refusal (below); `--preview` is the shared non-mutating usage error, there being no action to describe |
| `ai digest <account>` | the latest stored digest (migration 0010) beside the audit ledger | `digest`: the document's `content` whole with `period_start`, `period_end`, `created_at`, and the generation turn's `correlation`, or `null` when no generation stored one |
| `ai report <account>` | the latest stored analytics report (the same store's report kind) | `report`: the same fields under its own word; the kinds never bleed |
| `ai digest generate` | nothing: the spelling parses and refuses | exit `3` with the model rule's refusal (below); `--preview` is the shared non-mutating usage error, there being no action to describe |
| `ai report generate` | nothing: the spelling parses and refuses | exit `3` with the model rule's refusal (below); `--preview` is the shared non-mutating usage error, there being no action to describe |
| `knowledge list --account <id>` | the knowledge store (0007) beside the audit ledger | `entries`: id, kind, text, `source_account`, `source_key`, `created_at`, newest first; the entry's text rides whole, the whole-body discipline `ai drafts` carries |
| `knowledge forget --id <id>` | deletes one stored knowledge entry, the only exit | `id`, `state: forgotten`; `--preview` names the exact forgetting; an id no entry carries is a runtime error naming it |
| `knowledge extract` | nothing: the spelling parses and refuses | exit `3` with the never-dials refusal (below); `--preview` is the shared non-mutating usage error |
| `snippets list` | the snippet library (0008) beside the audit ledger | `snippets`: id, title, body, `created_at`, `updated_at`, the most recently touched first; the body rides whole, the insert places exactly it |
| `snippets create --title <t> (--body <text> or --body-file <path>)` | writes one snippet to the snippet store | `snippet` (id, title, body, the store's stamps) and `state: created`; `--preview` prints the exact snippet the store would take; the body comes from one source, two at once is a usage error |
| `snippets delete --id <id>` | deletes one stored snippet | `id`, `state: deleted`; `--preview` names the exact deletion; an id no snippet carries is a runtime error naming it |
| `snippets search` | nothing: the spelling parses and refuses | exit `3` with the never-dials refusal (below); the ranked search runs in the app's composer picker, and the agent filters `snippets list` itself |
| `completions <shell>` | nothing: the CLI's own verb vocabulary, no store read and no engine | the completion script raw by default (the `eval "$(astra-cli completions bash)"` idiom); under `--json`, `shell` and `script` (the script as a JSON string). An unknown or missing shell is a usage error |

Rules that bind every verb:

- **Account ids come from the index.** A verb that names an account fails
  with a runtime error when the id is not in the account index. The CLI
  never opens the OS keyring where each account's config (a credential)
  lives, so it can never know a password and never dial an account.
- **A folder-filtered list uses the app's own list policy.** `--folder`
  reads the windowed list over an over-reaching window (`limit` times four,
  floored at 500, the numbers `astra-app/src/window.rs` uses) and slices the
  folder's rows to `limit`, so one folder's page does not run short when the
  account's newest mail is filed elsewhere.
- **Bodies come from the store's cache.** `messages read` extracts the body
  from the cached source; a message whose body was never warmed in the app
  fails the verb with a runtime error rather than fetching over a network
  the CLI does not hold. The unsanitized HTML alternative is reported
  present (`html_present`) and never emitted: agents read the plain form,
  and rendering the HTML is a host concern.
- **A preview prints the action, from the one builder that runs it.**
  `drafts save --preview` prints the very draft the engine call would carry
  (addresses, subject, and the body's length, never its text); `drafts
  discard --preview` prints the exact deletion. The four `messages` act
  verbs print the exact `MailEdit` the engine call would carry, the same
  discipline: `mark-read` and `set-flagged` show the keyword change,
  `archive` and `trash` show the resolved destination. Nothing is written
  on a preview; the audit row's outcome says `preview`.
- **The act verbs stop at the row's trash.** `messages mark-read`,
  `set-flagged`, `archive`, and `trash` are the whole act family: no
  permanent-delete verb ships (the app layer's delete stays unexposed), and
  a key naming no message the store holds is a runtime error rather than a
  queued op, so an assistant cannot claim success over mail that never
  moved. The archive and trash destinations resolve the way the app layer
  resolves them (`astra-app/src/mail_writes.rs`): the SPECIAL-USE role,
  else a conventional folder name, else for Archive the `\All` mailbox; the
  two crates do not depend on each other, so the resolution is spelled
  twice and pinned by both test suites, the closed action set's own
  precedent.
- **The act verbs answer the durable op.** Every edit queues through the
  engine's outbox first: the answer says `applied` when the provider leg
  took it and `queued` when the op waits for the app's drain, the drafts
  verbs' own two-state answer. Offline the answer is always `queued`, and a
  keyword change shows in the store from that moment.
- **The body comes from one source.** `--plain`, `--file`, or stdin; two at
  once is a usage error.
- **The store verbs answer without the engine.** `rules`, `ai`,
  `knowledge`, and `snippets` read and write the rules, results,
  reply-drafting, documents, knowledge, and snippet tables (the draft
  pool and style profiles, the summarise cache, the digests and
  reports, the knowledge entries, the
  snippet library) beside the audit ledger in
  `ai.sqlite`, the one file every invocation already opens: no mail is
  read and no provider is dialled, so the verbs work against a cold data
  dir exactly as the mail verbs work against a synced one.
- **The CLI never dials a model.** `ai cold` and `ai results` return what
  a pass recorded, never a live call: a verdict exists here only because
  the app's incoming pass judged the message, and an unjudged message
  answers `null`, never a guess. `ai drafts`, `ai summary`, and `ai
  style` extend the same honesty to the reply-drafting tables: a
  pool, cache, or profile row exists only because the app's drafting
  driver or pulls recorded it. The generation seam is the same
  honesty: `rules create` takes an explicit condition sentence and
  generates nothing, because the background client lives in the desktop
  host and the CLI holds no model connection and never will. The agent
  composes the condition itself, through its own model, and the verb
  stores what it is given (the seam recorded in `docs/ai-rules.md`).
  `ai style rebuild` carries the rule to its refused edge: the rebuild
  is a model inference, so the spelling parses into the refusal below
  rather than pretend to infer. The A3 assist spellings carry it
  further (the never-dials rule, the contracts in
  `docs/ai-assist.md`): `knowledge extract` is a batch completion over
  the account's mail and `snippets search` is a ranked completion over
  the library, so both parse into the same refusal naming where the run
  happens; `knowledge list` and `snippets list` read what the app
  landed, and autocomplete and translate have no CLI spelling at all,
  being editor and reading-pane gestures. The A4 document spellings
  carry it the same way (the contracts in `docs/ai-digest.md`):
  `ai digest` and `ai report` read the documents the app's panel
  generated and stored, and the `generate` spellings parse into the
  refusal naming where the one windowed call runs.
- **The closed action set is spelled twice, by design.** The CLI's
  `CLOSED_ACTIONS` and the app layer's `Action::ALL` carry the same wire
  words in two crates that do not depend on each other; both are pinned
  by their own tests, and a change to either lands in both places. The
  duplication sentence and its cost live in `docs/ai-rules.md`.
- **The completion generator reads the vocabulary, not clap.**
  `completions <shell>` builds each script from `Verb::name()`, the
  subverb words, and a per-verb flag descriptor, an exhaustive match over
  `Verb` so a new verb cannot ship without completion vocabulary. Each
  script also completes the generator's own shell argument, offering the
  four shell words in the `completions` verb's second slot. The
  founding spec's clap_complete sentence is superseded: the parser is
  hand-rolled and clap-free, so the scripts generate from the existing
  vocabulary instead, the one-source-of-truth discipline the codebase
  pins by tests. The flag descriptor spells the parse arms' flags a
  second time, the closed action set's own precedent, and a drift test
  pins the two spellings together. The verb is non-mutating (the shared
  gate refuses `--preview`) and answers without the engine, the store
  verbs' discipline, so it works against a cold data dir.

## The envelope and the exit codes

Every invocation prints one JSON envelope on stdout, with the one
generator exception noted below. `--json` pins the
one-line form; without it a terminal gets the indented form and a pipe gets
the one-line form, so both readers always parse the same value.

```json
{"schema": 1, "ok": true, "data": { ... }}
{"schema": 1, "ok": false, "error": {"code": "usage", "message": "..."}}
```

- `schema` is pinned at `1`. It moves only as a contract change, with the
  sidecar's parser and the byte-pinned tests in the same change.
- `error.code` is one of `usage`, `runtime`, `refused`.
- The exit status follows the code: `0` ok, `1` runtime, `2` usage, `3`
  refused.

**The one generator exception: `completions`.** The envelope rule has a
single exception, the human-facing completion generator the sidecar never
calls. `completions <shell>` prints the raw script by default, the
`eval "$(astra-cli completions bash)"` idiom, and wraps it in the standard
envelope under `--json` (`data` carries `shell` and `script`, the script a
JSON string, schema still `1`). The agent-facing uniform envelope is
intact because the sidecar never invokes completions; every verb the
sidecar drives prints the envelope exactly as above.

**Send-class verbs are parsed and then refused.** `send` and `queue` (and
anything else later assigned to the send class) parse far enough to know
what was asked, then exit `3` with a message naming the human-send rule:
humans decide and send, the app sends. The refusal lives in the argument
parser, so no send-class spelling can ever reach the engine.

**Model-class verbs refuse the same way.** `ai style rebuild` would have
to infer the style note from Sent mail through a model completion; the
CLI holds no model connection and never will, so the spelling parses
into a refusal that names the rule and points where the rebuild runs:
the app's demand-driven profile read, where the next drafting turn or
style read after the daily cap passes infers a fresh note from Sent mail
(no explicit rebuild command ships; the contract and the recorded
deviation from the spec's mutating-verb sentence live in
`docs/ai-replies.md`). The verb ships no `--preview`, because there is no
action to describe. Exit `3`, ledger outcome `refused`, the row naming
the full `ai style rebuild` spelling. `knowledge extract` and `snippets
search` refuse identically (the A3 assist contracts live in
`docs/ai-assist.md`): the extraction runs as the host's
`knowledge_extract` command over the account's recent mail, the ranked
search as the composer picker's search, and the agent reads what landed
through `knowledge list` and `snippets list`. `ai digest generate` and
`ai report generate` refuse identically too (the A4 document contracts
live in `docs/ai-digest.md`): the generation runs as the panel's
Generate button over the account's windowed mail, and the agent reads
what landed through `ai digest` and `ai report`.

## The audit

Every invocation, whatever it did, appends one row to the AI ledger beside
the data dir (`ai.sqlite`): the verb, a redacted argument preview, the
outcome (`ok`, `error`, `usage`, `refused`, `preview`), and the turn's
correlation id, taken from `--correlation-id` or generated when absent. The
argument preview carries shapes only: account and folder ids, counts,
lengths. Addresses, subjects, and body text never enter the ledger. A line
that fails to parse records only the verb word, joined with a subverb when
the subverb is the CLI's own vocabulary: caller-typed positionals never
reach the ledger, and the correlation id already ties the whole line to its
turn. When the ledger cannot be opened or written the invocation is not
answered: the CLI prints a runtime error and exits `1`.

## The permission gating map

A virtual permission tool is a gated tool-surface entry whose execution
suspends behind a human approval card. The sidecar's bridge fails closed
on any tool absent from the policy table: an absent row means
needs-approval there. Adding a tool forces this map, the host's policy
table (`default_policy()` in `client/desktop/src/agent/mod.rs`), and the
preview vocabulary (`shaped_arg_preview` in
`client/desktop/src/agent/permissions.rs`, mirroring `cli/src/audit.rs`)
to answer for it, the exhaustive-match precedent `docs/plugins.md`
states for permissions.

| Tool | CLI verb | Level | Redacted preview shape |
|---|---|---|---|
| `list_folders` | `folders list` | `auto` | `account=<id>` |
| `list_messages` | `messages list` | `auto` | `account=<id> [folder=<key>] [limit=<n>]` |
| `read_message` | `messages read` | `auto` | `account=<id> keys=<n>` |
| `search_messages` | `messages search` | `auto` | `query_chars=<n> [account=<id>] [folder=<key>] [offset=<n>]` |
| `save_draft` | `drafts save` | `confirm` | `account=<id> from=<n> to=<n> cc=<n> subject_chars=<n> body_chars=<n>` |
| `discard_draft` | `drafts discard` | `confirm` | `account=<id> key=<id>` |
| `mark_read` | `messages mark-read` | `confirm` | `account=<id> key=<id> read=<bool>` |
| `set_flagged` | `messages set-flagged` | `confirm` | `account=<id> key=<id> flagged=<bool>` |
| `archive_message` | `messages archive` | `confirm` | `account=<id> key=<id>` |
| `move_to_trash` | `messages trash` | `confirm` | `account=<id> key=<id>` |

The four reads run; the six writes ask. The preview shape is the host's
`shaped_arg_preview` output for an ask, the redacted argument preview
the permission audit row carries: shapes and store ids only, never
addresses, subjects, or body text, the same vocabulary the CLI's own
audit rows carry. Optional fields ride only when the ask's args carry
them; the search preview carries no `limit`, because the tool spells
`--limit` on the argv but its approval args hold only the query, the
optional account and folder, and the optional offset. Three pins hold
the map: `policy_tests.rs` holds the policy table to exactly these ten
rows, `permissions_tests.rs` holds every preview shape, and the
sidecar's `mailToolsMap.test.ts` holds the ten names and every verb and
subverb.

Known gap, recorded rather than left silent: the `confirm_preview`
level and the approval `preview` payload exist end to end in the
protocol and the bridge, but nothing produces them yet. `runGated`
passes no preview to `requestApproval`, the sidecar never spells
`--preview`, `default_policy()` uses no `ConfirmPreview`, and the
approval card renders the ask's raw args, not the action preview.
Wiring the `--preview` output onto the card is a deeper slice (the
confirm_preview cluster), distinct from the redacted-args preview this
table documents.

## Where the stores live

`ASTRA_DATA_DIR` names the data dir; unset, the CLI uses the platform's
per-user data directory under the app identifier, the same directory family
the desktop host resolves through Tauri. Inside it the CLI opens the engine
store (`astra.sqlite`, the same file the app opens) and the AI ledger
(`ai.sqlite`, beside it).

**The second-writer seam, stated honestly.** The CLI runs beside a live
app against the same engine store. The store is WAL-mode SQLite with a
five-second busy timeout, and every CLI write goes through the engine's own
`put_draft`, `delete_draft`, and `edit_mail` under the outbox's lease and
resource serialisation, so two writers cannot corrupt one another. But the
app is not pushed: it learns a CLI-written draft or edit through its outbox
surface and its next sync or refresh, exactly as it learns a parked save of
its own.
The CLI's reads see the last synced state and never ask a server for more.
One deviation from a pure reader stands: the engine's `open` creates and
migrates the store even on a read path, so a first CLI run against a data
dir the app never made creates `astra.sqlite` and brings it to this
engine's schema. That is benign beside the app because both sides run the
same engine version and therefore the same migrations, and it is why the
read verbs never need a staged store to answer empty. This is the recorded
known interaction, not a gap to close.

## Matrix

| Rule | Status | Where |
|---|---|---|
| Thirty-two verbs with the pinned envelope | ✅ | `cli/src/verbs` |
| `--preview` on every mutating verb (messages act, drafts, rules, knowledge, and snippets), one shared builder each | ✅ | `cli/src/verbs/{messages,drafts,rules,knowledge,snippets}.rs` and `message_writes.rs` |
| Every invocation audited with a correlation id | ✅ | `cli/src/audit.rs`, `cli/src/verbs/mod.rs` |
| Send-class verbs parsed and refused, exit 3 | ✅ | `cli/src/verbs/mod.rs` |
| Model-class verbs (`ai style rebuild`, `ai digest generate`, `ai report generate`, `knowledge extract`, `snippets search`) parsed and refused, exit 3 | ✅ | `cli/src/verbs/{ai,knowledge,snippets}.rs` |
| Exit codes 0, 1, 2, 3 | ✅ | `cli/src/envelope.rs` |
| Redacted argument previews (shapes, never text) | ✅ | `cli/src/audit.rs` |
| Staged as the desktop bundle's `externalBin` | ✅ | `client/desktop/scripts/stage-cli.mjs` |
| `--json` on every verb of a versioned schema | ✅ | schema pinned at 1 |
| The CLI generates rules or dials a model | N/A | the generation seam: the agent composes, the CLI stores |
| Send-class verbs beyond refusal (none ship) | N/A | the send class is the refusal |
| The mail action verbs (mark-read, set-flagged, archive, trash) with the MCP search contract's scoping | ✅ | `cli/src/verbs/{message_writes,messages}.rs` |
| A permanent-delete verb (never ships; the row's trash is the ceiling) | N/A | the app layer's delete stays unexposed |
| Virtual permission tools over the verbs | ✅ | the permission gating map above: ten tools pinned to verb, level, and preview shape |
| Shell completions from the verb vocabulary (`completions <shell>`: bash, zsh, fish, pwsh) | ✅ | `cli/src/verbs/completions.rs`, the one generator exception to the envelope rule |

Known gaps, recorded rather than left silent:

- The CLI answers `accounts list` from the index file, because the engine
  store exposes no account enumeration at the pinned rev and the app keeps
  its account list in memory from the index at boot. A store-side account
  listing would supersede the file read if the engine grows one.
- `drafts save` cannot name the copy it supersedes: the verb takes no
  `--replacing`, so a second save of one composition stores a second draft
  beside the first until the reply-and-forward slice lands with its
  threading linkage.
- The default data dir is spelled with the identifier
  `com.kenizhou.astra` in both `tauri.conf.json` and the CLI's `main.rs`;
  the branding-time env unification is the desktop shell's, not the CLI's,
  and moves when that does.
