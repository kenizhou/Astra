Astra Mail now looks at your incoming mail with the AI foundation it
shipped earlier, and records what it concludes. After each refresh a
bounded pass judges the newest unread messages nobody has judged before:
your own plain-English rules are matched one by one (a matching rule then
marks the message read or flags it, through the app's own write paths,
never by the model itself), each message is sorted into one of five
categories, and a cold-outreach verdict is recorded for every sender. No
badge or inbox change rides the verdicts yet; they are queryable and
audited, every judgment landing in the AI ledger under one correlation id
per pass. What the model sees is guarded: `ASTRA_DLP` selects whether
mail content passes untouched (the default), has addresses, phone-shaped
numbers, and long digit runs masked before it leaves the machine, or is
refused outright, and a malformed value disables the pass loudly rather
than quietly allowing everything.

Platforms: desktop
Bump: minor
