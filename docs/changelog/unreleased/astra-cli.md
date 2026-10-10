Astra Mail now ships `astra-cli`, the command line an AI agent is allowed
to drive: it lists accounts, folders, and messages, reads a message's
cached body, saves and discards drafts through the mail engine's own
outbox, and nothing else. Every invocation answers in one JSON envelope
whose shape is pinned, the two draft verbs print a preview of the exact
write they would make when asked, and asking it to send or queue anything
is refused outright with a pointer to the rule that humans decide and
send. Each run, including refusals, leaves an audit row in the AI ledger
beside your data, tied to the conversation turn that caused it. The CLI
never touches your account credentials and never contacts a mail server:
a draft it saves parks in the outbox and the app stores it on the server
as part of its normal flow.

Platforms: desktop
Bump: minor
