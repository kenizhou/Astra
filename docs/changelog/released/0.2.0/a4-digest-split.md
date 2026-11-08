Astra Mail gains an AI panel over your recent mail and a split inbox
toggle. The status bar's AI button opens the panel: a digest of the
last few days of arrived mail (your own sent mail stays out of it) and
an analytics report with an executive summary, response patterns, and
recommendations, each generated on demand over a window you pick, one
to thirty-one days, and stored so reopening shows what was written
without paying again. Generating the same window again replaces it
quietly. The message list gains an All / Actioned / FYI switch with
live counts: mail the incoming AI pass filed as needing you lands under
Actioned, newsletters, notifications, receipts, and cold mail under
FYI, and everything else stays under All; the filter is local, instant,
and spends no model calls. `astra-cli` grows matching reads: `ai digest
<account>` and `ai report <account>` answer the latest stored documents
of their kinds, and the `ai digest generate` and `ai report generate`
spellings refuse as ever, because the CLI dials no model: the panel
runs the generation.

Platforms: desktop
Bump: minor
