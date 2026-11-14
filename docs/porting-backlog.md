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
| SP2k | The MCP server: the query layer, the tool surface, the pipe and relay, the Advanced panel | the MCP row |
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

## The lock-phase assessment (2026-11-03, all slices landed)

SP2k through SP2v are merged; the ladder is empty. The matrix counts
16 rows shipped, 101 partial with named shortfalls in their notes, and
13 unstarted, every one listed below for the owner to pick. The
shortfall-naming discipline held through every slice: no partial row
carries a silent gap, and the reviews enforced the naming at each
landing. The references can be locked and the 22:00 upstream-sync
activation scheduled.

The 13 unstarted rows, the owner's picking list: the outgoing name per
account (its core landed with SP2w; the surfaces follow); the JMAP and
IMAP in-place re-grants (both families' expired rows name the reconnect
remedy today); the unified inbox behind the account switcher; the vendor
account family (the next-device list, the sign-in, the subscription view,
decided out as N/A on 2026-11-07, the decisions below); the Drafts row
reopening into the composer; the image resize handle; the mail actions
row (read and unread, flag, archive, delete, trash); the offline diary at
launch and the periodic calendar refresh; the showcase mode (the fixtures
dev server already renders the seeded shell); and the assistant chat with
tool budget.

## The decisions this backlog owes

- **The Allodia commerce surface** (the vendor account, the subscription
  view, the next-device account list) is decided out: the three rows carry
  N/A in `capabilities.md` (2026-11-07), naming the licence bar
  (LicenseRef-Allodia-1.0 §4.2, the code published to be audited, not
  reused) and the owner's from-scratch path (the business behind the
  reference is Allodia's closed account service, so a port would be a
  from-scratch commercial service build, not a port). Reopening the family
  means the owner building a vendor service; it never returns as porting
  debt.
- **The kylins additions** (the command palette, pin and pinned searches,
  snooze, quick steps, sweep, unsubscribe, VIP, watch-replies, hover
  actions, flag reminders, junk blocking, categories and classification
  and conditional formats, quote collapse, shortcuts help, the attachment
  browser, notification history, the crypto plugin surface) each become
  their own slice beside this ladder. The owner delegated the picking to
  the loop (2026-11-12); the command palette, shortcuts help, hover
  actions, quote collapse, notification history, sweep, quick steps, pin,
  pinned searches, VIP, flag reminders, the attachment browser, junk
  blocking, categories, and watch-replies have shipped, and the rest follow
  in turn.
- **The AI remainder**: the assistant chat with tool budget was the one
  unstarted inbox-zero row; SP2y shipped it (2026-11-08, PR #60), live-proven
  armed over the harness and the model endpoint.

## Known gaps

- The slice ladder is an estimate of shape, not a promise of size: a slice
  may split or merge as its spec lands, and this table moves with it.
- The kylins additions are unscheduled by design; nothing above depends
  on them.
