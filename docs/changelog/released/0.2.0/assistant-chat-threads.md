The assistant chat keeps its threads. A prompt sent with no thread
selected starts one, titled from the prompt's first line, and every turn
records both sides (the rendered text, completed tool calls, and notes,
beside the run's metadata: model, tier, finish reason, and tool-call
count) in the local AI store. Each thread rides its own agent session,
the panel gains a thread list with select, rename, and delete, and a
loaded thread paints only what the store's rows hold: the runtime's own
session files stay engine-room. One prompt runs at a time per sidecar: a
second prompt while a turn is live is refused with an honest error
naming why, so the live turn's budget, correlation, and pending approval
are never disturbed.

Platforms: desktop
Bump: minor
