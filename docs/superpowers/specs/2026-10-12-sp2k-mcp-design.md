# SP2k MCP Server Design

Date: 2026-10-12
Status: proposed
Parent spec: `docs/superpowers/specs/2026-08-...-workflow-design` (sections 5, 15)
Port contracts: mail-calendar `docs/mcp.md` in substance, reconciled with Astra's
own security rulebook (the recorded deviations below).

## 1. Purpose

Letting an AI assistant on the user's own machine read and act on their mail
over the Model Context Protocol: desktop only, off by default, reaching
nothing until the user names the accounts it may touch, with writes going
through the same door the user does and reads through a door that changes
nothing.

## 2. Decisions

- **D1 (the transport)**: the two-process shape. An MCP client spawns a
  stdio relay (JSON-RPC 2.0, newline-delimited; a byte pipe with no
  protocol knowledge) that bridges to the app over a named pipe on Windows
  (a Unix domain socket on the other desktops, same code path); the app's
  listener owns the store, the live connections, and the credentials, so
  there is never a second writer.
- **D2 (the gate)**: off by default, in Settings, Advanced. The account
  allow list is empty by default and empty exposes nothing: an unlisted
  account is not even named to the client. The endpoint is reachable only
  by the signed-in OS user, with the peer verified. Turning the server on
  and granting an account are separate decisions.
- **D3 (the tool set)**: `list_accounts`, `list_folders`, `list_messages`,
  `search_messages`, `get_message`, `mark_read`, `set_flagged`,
  `archive_message`, `move_to_trash`, `create_draft`. snake_case,
  draft-2020-12 schemas, `additionalProperties: false`, annotations set
  honestly; `account` and `key` always travel together (the
  wrong-account-routing class stays unrepresentable at the boundary).
  **Three deviations against mail-calendar's contract, all recorded**: no
  `send_message` at all, not even behind a toggle, because Astra's
  rulebook refuses send-class verbs to the agent and humans decide and
  send (the escalation note lives in the PR); no `mark_as_spam` because
  Astra's dispatch carries no spam verb. `add_account`,
  `open_account_setup`, and `permanently_delete` stay cut for
  mail-calendar's own reasons. Third: mail-calendar persists the enable
  flag and the account allowlist across restarts; Astra re-closes the
  door at every boot (off and empty), because the rulebook is fail
  closed and this endpoint is the widest reach an assistant can obtain
  (the escalation notes live in the PR).
- **D4 (the two doors)**: writes dispatch the same `Intent`s the UI
  dispatches (an assistant's archive happens in the user's own list,
  visibly). Reads go through a query layer that changes nothing: no
  snapshot republish, no selection move, no query rewrite, and
  `OpenMessage`'s server-side mark-read never fires. Guarantee tests pin
  the split so a later change cannot collapse it.
- **D5 (the body rules)**: bodies come only from `get_message`, one at a
  time; plain text by construction; fenced in `<untrusted-message-content>`
  with a one-line preamble and a self-closing fence neutralized; no
  attachment bytes cross (names only); no irreversible primitive exists.
- **D6 (the ledger)**: every tool call lands in the audit ledger with a
  correlation id, as counts, ids, and durations: a tool name, an outcome, a
  duration, a query's character count. Never a query, an address, a
  subject, or a body.
- **D7 (create_draft)**: the draft opens in the app's own composer for the
  human, the agent-sidecar's approval-card seam reused; nothing sends.

## 3. Testing

- Unit (astra-app over the engine fakes): the query layer's guarantees (a
  read republishes nothing, marks nothing, moves nothing; paired with its
  contrast test stating why); the fence and the plain-text construction;
  the allowlist's empty-exposes-nothing (list_accounts answers nothing,
  not even a count); the tool schema literals.
- The server (in-process): the JSON-RPC framing, the tool dispatch, the
  error envelopes, the peer check, the ledger rows.
- E2E (gated): the relay binary bridged to the listener over the real
  pipe; a client's `initialize`/`tools/list`/`tools/call` round trip; the
  allowlist gate live; a write visible in the app's own list.
- Live (gated, harness): a `get_message` body over real IMAP mail; a
  `search_messages` over the seeded corpus.

## 4. Known gaps

- The relay binary ships for Windows first (named pipes); the Unix socket
  path compiles but is unproven until a Unix host CI exists.
- No `send_message`, no spam tool (the recorded deviations above).
- The composer's draft hand-off reuses the agent-sidecar's seam; a draft
  created while the composer is closed opens it on the next focus, not
  immediately (the panel's own timing).
- The allowlist lists accounts, not folders: an exposed account exposes
  every folder the app reads for it (mail-calendar's row-level gating
  arrives with its folder-level ACL work).
- The archive target resolves, in order: the RFC 6154 `\Archive` role, a
  conventional folder name (the leaf of a hierarchical name,
  case-insensitive, across the locales mail-calendar ships in), and only
  then the account's `\All` mailbox (the Gmail shape, where archiving is
  the absence of the Inbox label and All Mail is the archived message's
  home). Trash resolves the same way without the All fallback. The
  resolution is decided here and implemented once in astra-app, so the
  intent, the agent tool, and any future swipe share one answer.
- An agent's fresh draft opens under the compose-request snapshot's reply
  kind with no threading header: the snapshot vocabulary predates agent
  drafts and carries no "new" spelling, and the downstream composer keys
  its threading off the header, which is absent. An agent draft's `bcc`
  prefills nothing for the same reason (the snapshot has no Bcc field);
  the composer opens showing exactly what it will send, and nothing sends
  until the human presses send. Both narrow when the compose-request
  contract grows its agent shape.
- The port from the server to `App<P>` (the trait implementation in
  astra-mcp) is delegation-thin by design: its behaviour is pinned in the
  layers it delegates to (the query layer's and the writes' tests in
  astra-app, the bridge and server tests over the fake port), and the
  gated e2e of the proofs task drives it over the real pipe.
- The relay exits non-zero with a stderr diagnostic when the app is not
  reachable, instead of answering the JSON-RPC "unavailable" frame
  mail-calendar's shim answers: the frame needs the request id, and
  reading it is protocol knowledge this relay refuses to hold. The client
  reports a failed server; opening the app is the fix.
- The Unix transport half (the 0700/0600 permissions, the stale-socket
  claim, the peer-uid refusal) is implemented but pinned nowhere that
  runs: this host is Windows, and the `cfg(unix)` tests land with the
  first Unix CI.
