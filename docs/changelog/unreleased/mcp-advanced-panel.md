Adds the AI assistant access panel in Settings, Advanced: a toggle
that turns the local MCP server on and off, a per-account allowlist
naming the mail an assistant may reach (empty by default, and empty
exposes nothing), a reach sentence for the gate's current state, the
bind-failure diagnostic when the endpoint could not be taken, and a
client-config snippet carrying the relay command to paste into an MCP
client's config. The server starts every launch off with an empty
allowlist: nothing persists, and granting is per session.

Platforms: desktop
Bump: minor
