Astra Mail's AI decisions that come down to one bit now go through a
binary judge: a strict one-word question to the cheapest model tier,
answered yes, no, or an honest abstain. An abstaining question is asked
once more and then falls to a per-question safe default from the
safe-default table (privacy and security fail closed; evals fail; pure
UX stages such as cold mail stay put), so an uncertain model can never
tip a decision on its own. Every judgment is
recorded in the AI ledger beside its cost, and a weekly spend guard
(`ASTRA_LLM_NANO_BUDGET_MICROS`, on by default at a generous budget)
stops the cheap-tier calls for the rest of the week once the budget is
reached, recording the skip honestly. Judges gate user experience only;
authorisation stays deterministic.

Platforms: desktop
Bump: minor
