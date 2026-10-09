Typing an email address in account setup now looks the domain's settings up
(JMAP well-known, Thunderbird autoconfig and ISPDB, IMAP/SMTP SRV records, MX
fallback) and offers the found setup prefilled, a JMAP server or IMAP/SMTP
host and security among the choices; manual entry and a manual override of
the recommendation stay one step away, and a lookup that finds nothing falls
back to manual. Detection runs within five seconds and never sends the typed
address anywhere: only the domain travels.

Platforms: desktop
Bump: minor
