Astra Mail now keeps an AI ledger beside your mail store. Every model
call and every agent action lands in it: which use case asked, which
model answered, what it cost in tokens, how long it took, and which
conversation turn caused it, all under one correlation id per turn, so
a draft the agent saved can always be traced back to the prompt that
produced it and the approval that allowed it. The model setup arrives
through ASTRA_LLM_ environment variables (tiers, keys, and optional
endpoint overrides), the background client walks a configured fallback
chain on rate limits and transient errors before giving up, and a
self-test drives the real agent sidecar through a scripted conversation
with a stub CLI to prove the whole loop offline. Model keys never leave
the host process except in band to the sidecar, and nothing recorded
can read your mail.

Platforms: desktop
Bump: minor
