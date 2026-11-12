# Notification history design: the bell and its record

**Status.** Decided 2026-11-12. Slices branch from main at this commit.

**Scope.** The next kylins addition: a history of the new-mail
notifications, behind a bell in the shell. Astra already ships the whole
notification pipeline (the core queue, the delivery driver, the host bridge
to the OS plugin, the settings switch); what is missing is exactly a record
of what was notified and a panel to read it. It ports kylins's
notification-history shape against Astra's event/store/title-bar idioms. No
engine change, no new intent, no new IPC surface (ipc.ts stays at its cap).
The host emits the notified batch as a Tauri event; the web records it
device-local and renders a bell popover.

## D1: the recording path (host emit, web record)

**Host.** `NotifyBridge::deliver` (client/desktop/src/notify.rs) already
holds the `AppHandle` in windowed mode and gates on the settings switch and
the window focus. After the gate passes (the mail that is actually
notified, kylins's own discipline), it emits a `new-mail` Tauri event
carrying the drained batch, the same pattern as share.rs and mailto.rs
(`app.emit`). `NewMailNotice` and `NewMailBatch` (crates/astra-app/src/
notifications.rs) gain `Serialize` (additive serde, the hard rule); the
payload is the batch's messages plus its overflow count. The headless
`LogNotifier` path emits nothing (no window, no history). The emit never
suppresses the OS notification: a history failure is logged, not raised.

**Web.** A `newMailEvents.ts` mount (modeled on events/mailtoEvents.ts,
StrictMode-safe dispose), mounted in App.tsx, listens for `new-mail` and
records into a `notificationHistoryStore`: a zustand store persisted to
localStorage (the device-local idiom appearance.ts and the swipe actions
use), capped at 100 newest first. An entry carries the message id (account
+ key, the dedupe key), the account, the key, the sender, subject, and
preview when present, and the arrival time. Recording dedupes by id (a
re-notified message moves to the front, not a second row). The store offers
record (a batch), dismiss (one id), and clear. No new IPC surface, command,
or intent.

**The gate decision, recorded.** History records the mail that was
notified (after the enabled/focus gate), matching kylins: while the window
is focused or the switch is off, no notification fires and nothing is
recorded. This is a notification history, not a new-mail log; the honest
new-vs-changed judgement stays in the core queue, never reimplemented web
side.

## D2: the bell and the panel

A `BellIcon` joins components/icons.tsx (none exists today). A bell button
mounts in the TitleBar (the TITLE_ICON_CLASS button idiom) and opens a
`NotificationHistoryPanel` popover. The panel lists the entries newest
first: sender and subject (or a count-only line when neither is present),
the arrival time, and the account. Clicking an entry opens the message
through the existing `intents.openMessage({ account, key })` (the
MessageRef maps one to one onto the notice's account and key) and closes
the popover. Each entry has a dismiss action; a Clear all empties the
history; an empty state reads when nothing is recorded. v1 ships Focus,
Dismiss, and Clear (kylins's Mark read / Archive hover actions are deferred
until the thread is in the loaded list, its own honest-v1 compromise).

The panel is keyboard-reachable (the bell is a real button, the entries are
buttons, Escape closes the popover) and uses the standing focus-visible
ring and the shared popover/focus idioms.

## D3: the contract

docs/background-sync.md states the notification rules; the slice adds a
short history clause there (the host emits the notified batch, the web
records it device-local capped at 100, the bell reads it) and mirrors it to
zh. A new Sync-area capabilities row lands beside the notification row,
with its zh mirror. The porting-backlog "notification history" entry
resolves with the slice. A changelog fragment lands.

## Known gaps recorded, not hidden

- History records notified mail only (after the gate); a focused window or
  a disabled switch records nothing. A new-mail log of all arrivals is a
  different feature.
- The overflow count rides the event but v1 records the explicit messages;
  a count-only overflow entry is deferred.
- Mark read / Archive from the panel are deferred (kylins enables them only
  when the thread is loaded); v1 is Focus, Dismiss, Clear.
- History is device-local (localStorage), not synced across devices and not
  host-persisted; it rides the running web view, like the appearance and
  swipe preferences.
- No per-account filter chips in v1 (kylins has them); the panel lists every
  account's notifications newest first.

## Test-first

Every behaviour lands with its red. Rust: the notify_tests.rs recorder-sink
pattern pins that deliver emits the `new-mail` event with the batch when it
notifies, and emits nothing when the switch is off or the window is focused
(the headless path emits nothing). Web: the event-mount test (the
mailtoEvents.test.ts Tauri-sentinel pattern) records a batch into the store;
the store test pins record/dedupe-by-id/cap-100/newest-first/dismiss/clear
and the localStorage persistence; the panel test pins the list, click opens
the message through the mocked ipc (the exact openMessage payload), dismiss,
clear, the empty state, and Escape close. Any mid-test store mutation under
fake timers rides act(). No live model proof; the offline battery is the
proof.