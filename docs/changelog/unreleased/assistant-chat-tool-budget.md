The assistant chat's turns now run under a wall-clock tool budget. The
clock starts when a prompt is received; past the bound the assistant's
tool calls are answered with a skipped outcome (the audit row lands as it
does for a refused tool, and the model is told why), and the assistant
finishes in text. A watchdog caps each turn at the budget plus a grace
period, so a turn always settles with its done event rather than hanging
the panel. The panel notes when the bound bites and its status line says
the assistant is finishing in text.

Platforms: desktop
Bump: minor
