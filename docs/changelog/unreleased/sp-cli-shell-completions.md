The CLI ships shell completions: `astra-cli completions <shell>` generates
a completion script for bash, zsh, fish, and pwsh from the CLI's own verb
vocabulary, so every verb and its flags complete and a new verb cannot
ship without completion vocabulary. The script prints raw by default (the
`eval "$(astra-cli completions bash)"` idiom) and rides the standard
envelope under `--json`, the one generator exception to the envelope rule;
the sidecar never calls it. The verb is non-mutating, answers without the
engine, and is audited like every invocation.

Platforms: desktop
Bump: minor