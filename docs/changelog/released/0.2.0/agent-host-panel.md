Adds the agent panel and the sidecar host behind it: an Agent toggle in
the status bar opens a chat panel whose prompts stream replies from the
supervised Node sidecar, whose tool calls surface approval cards that
route the answer back, and whose abort button cancels the live turn.
Every prompt carries a correlation id into the sidecar's tool spawns and
both ledgers (ai.sqlite), the host restarts a dead sidecar with a capped
linear backoff before giving up loudly, and unanswered approvals fail
closed on death, abort, and shutdown. The sidecar bundle is bundled as an
application resource and runs on Node 22 or newer from PATH or
ASTRA_AGENT_NODE.

Platforms: desktop
Bump: minor
