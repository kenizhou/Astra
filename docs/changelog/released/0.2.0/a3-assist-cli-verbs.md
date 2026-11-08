`astra-cli` grows seven verbs for the AI assist features. `knowledge
list` reads an account's extracted knowledge entries with their sources,
and `knowledge forget` removes one by id, the only exit, with a preview
of the exact forgetting first. `snippets list`, `snippets create`
(title with the body inline or from a file), and `snippets delete`
manage the snippet library the composer's picker shares, the mutating
verbs previewing what they would store or remove. The model-backed
spellings refuse as ever: `knowledge extract` and `snippets search`
name the rule and point at the app, because the CLI dials no model.
Every invocation lands its audit row as always.

Platforms: desktop
Bump: minor
