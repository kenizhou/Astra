# Porting backlog: the road to feature alignment

**Status.** A living roadmap, not a contract: it names what remains before
the reference baselines can be locked and the nightly upstream sync
activated. Row-level truth lives in `capabilities.md`; this doc assigns the
rows to slices and records the order. It moves in the same change as a
slice that retires part of it.

**The scale, honestly counted.** mail-calendar ships 132 capability rows,
all shipped. Astra today: a handful shipped clean, most partial, and a
long tail not started. kylins carries a further class of features Astra's
matrix does not yet name; per the founding rules mail-calendar wins
feature contracts, so those are additions the owner picks, not parity
debt.

## The slices, in order

| Slice | Scope | Retires |
|---|---|---|
| SP2k (in flight) | The MCP server: the query layer, the tool surface, the pipe and relay, the Advanced panel | the MCP row |
| SP2l | The OAuth foundation: browser PKCE sign-in, token storage and refresh, the reconnect banner, expired-sign-in reporting | the OAuth scaffolding rows |
| SP2m | Microsoft Graph: mail read and actions, send, calendar read and write, the directory | the Microsoft rows |
| SP2n | Google native APIs: mail and calendar | the Google rows |
| SP2o | JMAP: mail with bodies, send, calendar read; JMAP OAuth and renewal (the harness speaks JMAP, so this is live-provable the day it lands) | the JMAP rows |
| SP2p | The mail batch: folder management (create, rename, move, delete, drag), attachments both ways, multi-select actions, the unified All Accounts group, move-to-folder | the folder-pane and list-selection rows |
| SP2q | The composer finishing: the formatting toolbar, links, inline and attached files, signatures with per-account defaults, the From picker and default send account, mailto: entry | the composer rows |
| SP2r | The delivery batch: IMAP IDLE push and polling, background sync, new-mail notifications, offline-first bodies, per-account fetch depth and size | the sync rows |
| SP2s | The reading batch: the message own-window, Save as .eml, print, reading zoom and reflow, spam reporting, swipe actions | the reading rows |
| SP2t | The platform batch: diagnostics and the log viewer, consented analytics, the privacy surface, OS integration, localisation, the device-zone prompt | the platform rows |
| SP2u | The calendar and contacts finishers: the attendee list, full rule read-back, multi-day banners, the persisted shape, contact photos on mail rows | the scattered rows |
| SP2v | IMAP/SMTP OAuth sign-in where servers offer it | the mail-oauth row |

After SP2v: the lock-phase assessment (every row shipped, partial with
recorded gaps, or explicitly decided), then the 22:00 upstream-sync
activation.

## The decisions this backlog owes

- **The Allodia commerce surface** (the vendor account, the subscription
  view, the next-device account list) is mail-calendar's own business
  layer. Astra records an explicit N/A, or an owner decision to port,
  before the lock phase; it must not stand as a silent unstarted row.
- **The kylins additions** (the command palette, pin and pinned searches,
  snooze, quick steps, sweep, unsubscribe, VIP, watch-replies, hover
  actions, flag reminders, junk blocking, categories and classification
  and conditional formats, quote collapse, shortcuts help, the attachment
  browser, notification history, the crypto plugin surface) wait for the
  owner's pick; each pick becomes its own slice beside this ladder.
- **The AI remainder**: the assistant chat with tool budget is the one
  unstarted inbox-zero row; it slots wherever the owner places it.

## Known gaps

- The slice ladder is an estimate of shape, not a promise of size: a slice
  may split or merge as its spec lands, and this table moves with it.
- The kylins additions are unscheduled by design; nothing above depends
  on them.
