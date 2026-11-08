`astra-cli` grows five verbs for the AI rules feature. `rules list`,
`rules create`, and `rules delete` manage the stored rules the incoming
pass matches against: creation takes an explicit name, condition
sentence, and actions drawn from the closed set (mark read, flag), with
a preview of the exact rule before it is stored, and no generation, the
agent composes the condition itself. `ai cold` and `ai results` read
back what a pass concluded for a message: its stored cold verdict or
every recorded stage row, never a live model call, and `null` when no
pass has judged it yet. Every invocation lands its audit row as always.

Platforms: desktop
Bump: minor
