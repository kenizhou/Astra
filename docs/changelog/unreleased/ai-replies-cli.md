`astra-cli` grows three read verbs for the reply drafting feature. `ai
drafts <account> <key...>` answers every stored draft pool row for a
message (the whole reply text, its confidence, and when it was
generated, with a skipped marker where a generation failed); `ai
summary` answers the cached summary beside the body hash it was cached
under, or null when nothing is cached; `ai style` answers the account's
stored style note or null. None of them dials a model: rows exist only
because the app's drafting driver or pulls recorded them. `ai style
rebuild` parses and refuses (exit 3): a rebuild is model work that runs
in the app's demand-driven profile read, and the refusal says so.

Platforms: desktop
Bump: minor
