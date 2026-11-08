# Engine sync 68eab00 -> 73650cc (13 upstream commits)

Platforms: desktop
Bump: minor

- The first vendor-sync landed (PR #40, merge commit): the engine advances
  13 upstream commits. What the product gains:
  - Calendar invitation writes across providers (#197), and write receipts
    that say when a calendar or contact write was actually applied rather
    than merely accepted (#283).
  - DAV credential confinement: a credential can be confined to the hosts
    it is permitted for, a DAV token is sent as Bearer only (never as a
    Basic password), and it is presented the way the server's challenge
    asks.
  - JMAP sync correctness: the snapshot walk pages by anchor so a live
    account can no longer hide an object behind capped pages or a lost
    anchor, a paged snapshot is stored only once its query held still,
    mailboxes page, event creates declare JSCalendar 2.0, and whole-scope
    syncs stay within maxObjectsInGet.
  - A draft with no recipients stores without a `To` header, and an
    HTML-only message's text leg no longer leaks stylesheet contents
    (#275).
- Verified on the review merge: full workspace battery, xtask gate, the
  live harness (33/33 serial), and the engine battery inside
  `vendor/engine`, all green.
