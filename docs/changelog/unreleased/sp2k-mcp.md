AI assistants can now read and act on your mail through the Model
Context Protocol. A local MCP server, off by default, exposes ten tools:
listing accounts, folders, and messages, searching, reading one message
in full, marking read and unread, flagging, archiving, moving to trash,
and opening a draft in the app's own composer. Nothing sends: there is
no send tool, and a draft waits for you to press send. A message body
crosses only in the reading tool, as plain text inside an
untrusted-content fence that tells the assistant whose words those are.
Reads go through a path that changes nothing, so an assistant reading a
message never marks it read or moves your selection; writes go through
the same door your own clicks use, so their effects land in your own
list.

An Advanced settings panel owns the decisions: a toggle for the server,
a per-account allowlist that is empty by default and exposes nothing
when empty, and a client-config snippet naming the relay command. The
state never persists: every launch starts with the server off and the
allowlist empty, and you re-open both by hand. Every tool call is
recorded in the AI ledger with its own correlation id, the tool name,
the outcome, and the duration; never a message's content. The relay an
MCP client spawns is a byte pipe with no protocol knowledge; it is not
yet bundled beside the installed app, so the snippet's command resolves
in dev builds until the packaging pass.

Platforms: desktop
Bump: minor
