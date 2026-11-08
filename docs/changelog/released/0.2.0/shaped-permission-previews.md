Permission decisions in the AI ledger now record a redacted preview in
the same shape vocabulary the CLI uses (account, counts, and text
lengths) instead of a truncated copy of the tool's raw arguments, so
addresses, subject lines, and draft text never enter the ledger. Two
writers opening the AI ledger for the first time no longer race over
its migration bookkeeping: whichever opener loses the race continues
on the winner's schema instead of failing.

Platforms: desktop
Bump: none
