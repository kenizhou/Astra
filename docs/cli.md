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

Six ship today, one per surface that exists. Each takes flags after two
positional words (`verb subverb`), so a call is unambiguous in text and in
an audit row.

| Verb | Reads | Answer's `data` |
|---|---|---|
| `accounts list` | the account index (`accounts.json`) in the data dir | `accounts`: the ids, in index order |
| `folders list --account <id>` | the engine store's mailbox objects | `folders`: id, name, role, parent, `unread_count`, in the sidebar's canonical order |
| `messages list --account <id> [--folder <key>] [--limit <n>]` | the engine store's windowed list read | `messages`: key, from, subject, date, unread, draft, has attachment, preview |
| `messages read --account <id> --key <k> [--key <k>...]` | the engine store's whole-message read and cached body | `messages`: the envelope plus `body.plain` and `body.html_present` |
| `drafts save --account <id> --from <a> --to <a,...> [--cc <a,...>] --subject <s> (--plain <text> or --file <path> or stdin)` | writes through the engine's `put_draft` | `message_id`, `op`, `state` (`stored` or `queued`), and `key` when stored |
| `drafts discard --account <id> --key <k>` | writes through the engine's `delete_draft` | `key`, `op`, `state` (`removed` or `queued`) |

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
  discard --preview` prints the exact deletion. Nothing is written on a
  preview; the audit row's outcome says `preview`.
- **The body comes from one source.** `--plain`, `--file`, or stdin; two at
  once is a usage error.

## The envelope and the exit codes

Every invocation prints one JSON envelope on stdout. `--json` pins the
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

**Send-class verbs are parsed and then refused.** `send` and `queue` (and
anything else later assigned to the send class) parse far enough to know
what was asked, then exit `3` with a message naming the human-send rule:
humans decide and send, the app sends. The refusal lives in the argument
parser, so no send-class spelling can ever reach the engine.

## The audit

Every invocation, whatever it did, appends one row to the AI ledger beside
the data dir (`ai.sqlite`): the verb, a redacted argument preview, the
outcome (`ok`, `error`, `usage`, `refused`, `preview`), and the turn's
correlation id, taken from `--correlation-id` or generated when absent. The
argument preview carries shapes only: account and folder ids, counts,
lengths. Addresses, subjects, and body text never enter the ledger. When
the ledger cannot be opened or written the invocation is not answered: the
CLI prints a runtime error and exits `1`.

## Where the stores live

`ASTRA_DATA_DIR` names the data dir; unset, the CLI uses the platform's
per-user data directory under the app identifier, the same directory family
the desktop host resolves through Tauri. Inside it the CLI opens the engine
store (`astra.sqlite`, the same file the app opens) and the AI ledger
(`ai.sqlite`, beside it).

**The second-writer seam, stated honestly.** The CLI runs beside a live
app against the same engine store. The store is WAL-mode SQLite with a
five-second busy timeout, and every CLI write goes through the engine's own
`put_draft` and `delete_draft` under the outbox's lease and resource
serialisation, so two writers cannot corrupt one another. But the app is
not pushed: it learns a CLI-written draft through its outbox surface and
its next sync or refresh, exactly as it learns a parked save of its own.
The CLI's reads see the last synced state and never ask a server for more.
This is the recorded known interaction, not a gap to close.

## Matrix

| Rule | Status | Where |
|---|---|---|
| Six verbs with the pinned envelope | ✅ | `cli/src/verbs` |
| `--preview` on both mutating verbs, one shared builder | ✅ | `cli/src/verbs/drafts.rs` |
| Every invocation audited with a correlation id | ✅ | `cli/src/audit.rs`, `cli/src/verbs/mod.rs` |
| Send-class verbs parsed and refused, exit 3 | ✅ | `cli/src/verbs/mod.rs` |
| Exit codes 0, 1, 2, 3 | ✅ | `cli/src/envelope.rs` |
| Redacted argument previews (shapes, never text) | ✅ | `cli/src/audit.rs` |
| Staged as the desktop bundle's `externalBin` | ✅ | `client/desktop/scripts/stage-cli.mjs` |
| `--json` on every verb of a versioned schema | ✅ | schema pinned at 1 |
| Send-class verbs beyond refusal (none ship) | N/A | the send class is the refusal |
| Archive and label verbs | ⬜ | arrive with their slices |
| Virtual permission tools over the verbs | ⬜ | the sidecar's bridge, SPA0 later tasks |

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
