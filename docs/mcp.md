# Agent (MCP) access: the Astra Mail contract

**Scope.** The SP2k Model Context Protocol surface: the stdio relay, the
named-pipe listener behind it, the tool catalogue with its untrusted-body
fence, the account gate, the tool-call ledger, and the Advanced settings
panel that owns the user's decisions. Binding on every surface that ships
any of it (today the desktop host). The port source is mail-calendar's
`docs/mcp.md` in substance; the three recorded deviations below are the
whole difference in contract, and Astra's security rulebook decided each.

**Principle.** *Writes go through the same door the user does. Reads take
a different one.* A write dispatches the same write method a click
dispatches, so an assistant's mark-read happens in the user's own list,
visibly, and reaches the server on the same pass. A read must not, for two
reasons. Every read-shaped `Intent` ends in a snapshot rebuild, and
`Search` also rewrites the active query and scope, so "what is in my
inbox" would scroll and re-scope the window of a person reading something
else. And `OpenMessage` marks the message read on the server, so "read me
that email" would silently clear an unread badge. Reads therefore go
through the query layer (`App::query_*`), which changes nothing: no
republish, no selection move, no keyword written, and the UI's cache
untouched. Guarantee tests in `crates/astra-app/src/query/` pin the split
so a later change cannot collapse it back into one path.

## The transport

```
  MCP client (Claude Desktop, Claude Code, Cursor, ...)
        | stdio (JSON-RPC 2.0, newline-delimited)
        v
  astra-mcp-relay
  a byte relay, zero dependencies, no protocol knowledge
        | named pipe (Windows) / Unix domain socket (the same code path)
        v
  Server                                   crates/astra-mcp
        |-- reads        --> App::query_*  (stateless, non-mutating)
        |-- writes       --> App's own write methods (the ones intents dispatch)
        `-- create_draft --> the compose-request surface (nothing sends)
```

**Why two processes.** An MCP client spawns its server as a child and
talks stdio. But the running app owns the store, the live IMAP
connections, and the in-memory credentials: a standalone server process
would mean a second writer on one store and a second copy of the user's
secrets. The relay gives the client the stdio it expects while the app
stays the single owner, and what crosses between them is bytes.

**The relay is its own crate, links nothing of the mail stack, and knows
no protocol.** Not the frame, not the version, not the id: it copies
stdin to the endpoint and the endpoint to stdout, verbatim, so N bytes in
produce exactly N bytes out, binary garbage included. Nothing but
relayed bytes ever reaches stdout and every diagnostic goes to stderr,
because one stray line on stdout desynchronizes the client's parser.
This is why the relay is not a `[[bin]]` inside `astra-mcp`: a binary
target there would link the app, the engine, and SQLite into a helper
another application executes on the user's behalf. Both directions are
concurrent by construction (tokio's overlapped-I/O clients), pinned by
tests whose whole point is the *second* message: a Windows pipe serialises
synchronous I/O, so a relay that parks its read on the pipe blocks its
own write, answers exactly one request, and then hangs every client; the
tests send the notification a real client opens with (which draws no
reply) and then a further request, and both must go through.

**The unreachable app is a recorded deviation from mail-calendar's
shim.** The shim reads enough of the frame to answer a JSON-RPC
"unavailable" error when the app is not running; reading the request's id
is protocol knowledge this relay refuses to hold. So the relay exits
non-zero with the diagnostic on stderr and stdout untouched, the client
reports a failed server, and opening the app is the fix. Deliberate, and
pinned by test.

**Why no token.** The endpoint is a named pipe on Windows and a
`0700`-directory socket on Unix: the OS user boundary is the
authenticator, and there is no secret to generate, store, show, paste,
rotate, or keep out of a backup. A loopback HTTP port would need one, and
it would be load-bearing rather than defence in depth, because any local
process can reach a port. On Unix every accepted connection's peer uid is
checked against the socket file's owner. On Windows the boundary is the
pipe's own security descriptor, honestly stated: an instance created with
the default DACL serves the creating user, SYSTEM, and Administrators,
`reject_remote_clients` refuses the SMB path in from another machine, and
`first_pipe_instance` turns a name collision into a loud bind failure
rather than a hijack. There is no token or SID comparison code because
reading the connecting process's token has no safe API on the pinned
tokio, and the workspace forbids `unsafe` outright; the negative halves
(another user refused, a remote client refused) are enforced by the
operating system against that descriptor and are recorded as uncovered
here, not claimed as tested.

**The endpoint is derived once, by the host, never by the relay.** On
Windows the pipe name is the app data directory's final component plus
`.mcp`, so the standard directory yields `\\.\pipe\com.kenizhou.astra.mcp`
and a dev data directory beside it yields a different pipe: two builds on
one machine never steal each other's clients. On Unix the socket is
`mcp.sock` inside the data directory, its parent `0700` and the socket
itself `0600`. The client's config snippet carries the exact value the
host derived, and the relay takes it as an argument, verbatim, so the two
can never disagree about where to meet.

## The gate

Off by default, and the user goes looking for it: the toggle sits in
Settings, Advanced (the panel's own contract is `settings.md`). Turning
the server on and granting an account are two separate decisions, and
the account allowlist is empty by default. Empty exposes nothing:
`list_accounts` answers an empty list, not even a count of what exists,
and an unlisted account is refused on every account-scoped tool with a
message that does not distinguish "not exposed" from "no such account",
because which other mailboxes exist is itself something the user did not
agree to disclose. An unscoped search runs over exactly the exposed
accounts, so a caller that may see only some accounts is never answered a
superset.

The list is read fresh on every call, from one shared place, never
captured when the connection was accepted: a tick reaches an assistant
already connected, and an untick revokes its access immediately.

**The third recorded deviation: the door re-closes at every boot.**
mail-calendar persists the enable flag and the account allowlist across
restarts; Astra persists neither. Every boot starts with the server off
and the gate empty, and the user re-opens both by hand, because the
rulebook is fail closed and this endpoint is the widest reach an
assistant can obtain. The escalation note rides the slice's PR.

## The shared bar

Every surface that ships this meets all of it.

1. **Off by default.** The user goes looking for it in Settings,
   Advanced; there is no prompt.
2. **The account allowlist is empty by default**, and empty exposes
   nothing. Turning the server on and granting access to a mailbox are
   two separate decisions. An account that is not exposed is not even
   named to the client.
3. **No send verb exists at all, not even behind a toggle.** This is the
   first recorded deviation, decided by the rulebook: send-class verbs
   are refused to the agent, and humans decide and send. `create_draft`
   is the whole story: the draft opens in the app's own composer and
   nothing leaves the device until the human presses send. There is no
   direct-send toggle to turn on, no known-recipient guard to configure,
   and nothing to get wrong.
4. **No spam tool.** The second recorded deviation: Astra's dispatch
   carries no spam verb, so none is advertised. `add_account`,
   `open_account_setup`, and `permanently_delete` stay cut for
   mail-calendar's own reasons: a credential must never cross this
   channel, a popped setup window is a phishing primitive, and no agent
   gets an irreversible primitive.
5. **Bodies come only from `get_message`**, one message at a time. A
   listing or a search returns subject, sender, date, and flags. A
   search that returned bodies could drop fifty hostile texts into a
   model's context in one call, and it only takes one landing.
6. **Bodies are plain text by construction**, not by policy: the query
   layer converts them, so a caller over the message type structurally
   cannot emit HTML it was never given. HTML is a strictly larger
   injection surface (hidden spans, white-on-white, CSS `content`), none
   of which sanitisation removes.
7. **Bodies are fenced** in `<untrusted-message-content>` with a
   one-line preamble, and a body that tries to close the fence itself is
   neutralised.
8. **No attachment bytes cross the boundary**: names only.
9. **No irreversible primitive exists.** `move_to_trash` ships;
   `permanently_delete` does not.
10. **The endpoint is reachable only by the signed-in OS user**, as the
    transport section states and honestly bounds.
11. **Every call is ledgered** in `ai.sqlite` (`tool_calls`, migration
    `0011`): the call's own correlation id, the tool's name, the
    outcome, the duration, and the argument character count. Never a
    query, an address, a subject, or a body. A row that cannot land
    fails its call closed with an internal error rather than passing
    unrecorded.

### What this does not defend against, said plainly

A message body is text an attacker wrote, entering a model's context.
That cannot be fixed, only bounded, and the items above are bounds of
different strength. The fence is the weakest: it is a suggestion to a
model. `create_draft` is a human-visible step, not a safety guarantee: a
user who asked for "reply to Bob" will press send without reading. What
Astra's shape does give is the hard bound the rulebook drew: there is no
send tool to talk your way past, so the moment of dispatch stays a human
hand on a button the app owns.

And one thing "no token" must not be read as implying. Any same-user
process can already read the app's data directory, so this grants no
authority that was not already available. But it is a step change in
reachability: reading the store directly needs a deliberate attacker who
understands the schema, while a documented endpoint with a published
tool list is a discoverable API. That is a real difference and it is the
honest way to describe it.

Finally: whatever the connected assistant reads, its own provider may
also receive, under their policy. If it is a cloud assistant, message
content leaves the device the moment the user asks it to read something.
That is the user's dispatch to a party they chose.

## The tool set

`snake_case`, draft-2020-12 schemas, `additionalProperties: false`, and
annotations set honestly, because a client shows its confirmation prompt
off them. `account` and `key` always travel together, mirroring
`MessageRef`: a provider key is unique only within an account, so the
wrong-account routing class stays unrepresentable at this boundary too.
Every message-carrying result echoes both back.

| Tool | Input (required in bold) | Annotations |
|---|---|---|
| `list_accounts` | nothing | `readOnly` |
| `list_folders` | **`account`** | `readOnly` |
| `list_messages` | **`account`** `folder` `unread_only` `offset` `limit` (1 to 50) | `readOnly` |
| `search_messages` | **`query`** `account` `folder` `offset` `limit` | `readOnly` |
| `get_message` | **`account`** **`key`** | `readOnly` |
| `mark_read` | **`account`** **`key`** **`read`** | `idempotent` |
| `set_flagged` | **`account`** **`key`** **`flagged`** | `idempotent` |
| `archive_message` | **`account`** **`key`** | `idempotent` |
| `move_to_trash` | **`account`** **`key`** | `destructive`, `idempotent` |
| `create_draft` | `account` **`to`** list `cc` list `bcc` list **`subject`** **`body_text`** `reply_to` | none |

Reading a message never marks it read: `get_message` rides the query
layer, and the e2e proof asserts the row stays unread across a refresh
after the read.

## Enforcement

Automated:

- `crates/astra-app/src/query/` and its suites: the two guarantees (a
  read republishes nothing and writes no keyword), paired with the
  contrast that `OpenMessage` does write `$seen`, and the windowed-page
  contract.
- `crates/astra-mcp/src/fence_tests.rs`: the fence, its preamble, and
  the neutralised self-closing tag.
- `crates/astra-mcp/src/tools_tests.rs` and `bridge_tests.rs`: the
  schema literals against the input types, the gate's
  empty-exposes-nothing, the no-body listing rule, the page clamps, and
  the absent send and spam tools as explicit pins.
- `crates/astra-mcp/src/server_tests.rs` and `ledger_tests.rs`: the
  framing, the counter-offer handshake, the error envelopes, the ledger
  row's shape (counts and ids only, multi-byte characters included), and
  the fail-closed refusal when a row cannot land.
- `crates/astra-mcp/src/transport_tests.rs` and
  `transport_pipe_tests.rs`: the endpoint derivation and the malformed-id
  leak pin over the real pipe.
- `crates/astra-mcp-relay/tests/relay.rs`: over the built binary, a real
  pipe, and real stdio: the byte-count pin, the round trip whose second
  message is the point, the unreachable-app deviation, and the usage
  error.
- `client/desktop/src/mcp_tests.rs`: the host's run/stop seam, the
  taken-name failed state, and the allowlist seam reaching the gate on a
  live connection.
- `crates/astra-mcp-relay/tests/e2e.rs`: the gated end-to-end and live
  proofs, over the running harness. The relay bridging stdio to the
  pipe, the server behind it over a REAL two-account app (the product's
  own certificate-acceptance dial), and through that whole stack: the
  handshake, the ten-tool catalogue with the send cut visible as an
  absence, the folder names the pane itself uses, the three seeded notes
  newest first with no body on any row, the fenced plain text of one
  seeded note (and the row it did not mark read, across a refresh), the
  engine's real search through the gate's set form, a `mark_read` whose
  flip shows in the app's own list and survives a refresh, the restore
  that leaves the corpus as found, and the unlisted configured account
  refused through relay, server, and gate alike, unnamed by
  `list_accounts`, with the same refusal words as an id nobody holds.

Manual, per release, on the desktop:

1. `scripts/dev/harness.sh up`, boot the desktop, connect the harness
   account, and sync once.
2. Settings, Advanced: turn it on, expose the harness account, copy the
   snippet, and confirm the status line says running.
3. Paste the snippet into a real MCP client's config, restart it, and
   confirm the handshake and tool discovery in its logs.
4. One full loop: list the inbox, read one message and assert it stays
   unread in the app, mark it read and watch the row flip, draft a reply
   and assert a prefilled composer opens, unsent. Make at least two tool
   calls in one session: a relay that answers exactly one request looks
   healthy until the second.
5. The negatives: turn it off and confirm the next call fails cleanly;
   unexpose every account and confirm calls report nothing; quit the app
   and confirm the client shows the server as unavailable rather than
   wedged.
6. `scripts/dev/store.sh` or the ledger read of the release pass:
   confirm the reads left no trace in the store, and that every call
   left exactly one ledger row holding no content.

## Known gaps

- **The relay is not bundled beside the installed app.** The snippet
  derives a relay path the desktop bundle does not yet stage; the
  packaging pass owns landing it. The dev build finds the binary where
  cargo built it, and the e2e proof spawns that binary, so the contract
  itself is proven; only the installed-build placement is owed.
- **The Unix transport half is implemented but unpinned.** The `0700`
  directory, the `0600` socket, the stale-socket claim, and the peer-uid
  refusal compile on Unix; this host is Windows, and the `cfg(unix)`
  tests land with the first Unix CI. Writing unrunnable tests blind is
  not test discipline.
- **The Windows peer-check negatives are OS-enforced, not test-covered.**
  Another user and a remote client are refused by the pipe's descriptor;
  reproducing either needs a second account or a second machine, which
  this harness has neither of.
- **The allowlist lists accounts, not folders.** An exposed account
  exposes every folder the app reads for it.
- **An agent's draft rides the compose-request snapshot's reply kind.**
  The snapshot vocabulary predates agent drafts and carries no "new"
  spelling, so a fresh agent draft opens with no threading header, and
  its `bcc` prefills nothing for the same reason. Both narrow when the
  compose-request contract grows its agent shape. Nothing sends either
  way: the composer opens showing exactly what it will send.
- **A draft created while the composer is closed opens on the next
  focus, not immediately**: the agent-sidecar's seam and its timing, not
  a new one.
- **The port over `App<P>` is delegation-thin by design.** Its behaviour
  is pinned in the layers it delegates to and driven end to end by the
  gated e2e over the real pipe.
