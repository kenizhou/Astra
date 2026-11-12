# Pin design: floating pinned rows to the top

**Status.** Decided 2026-11-12. Slices branch from main at this commit.

**Scope.** The next kylins addition: pin, floating chosen rows to the top
of the list. Kylins's pin is entirely client-side (a persisted id set plus
a pure partition at render); Astra ports that shape at the list's single
render choke point. It is view state, like density or the filter tab: no
engine write, no new intent, no wire type, no ipc.ts change, so no eval and
no pipeline bump. The pin set is keyed by the row identity (account + key),
a deliberate per-account improvement over kylins's global bare-id set.

## D1: the pin set and the overlay

A `pinnedRowsStore` (zustand + localStorage, the notificationHistoryStore /
quickStepsStore idiom: defensive parse, persist logged never thrown,
hydrate at load). The set holds row identities (`rowKeyId`:
`message:account:key` or `thread:account:thread_id`), so pins are scoped per
account for free. The store offers `toggle(key)` (optimistic) and
`isPinned(key)`.

A pure, stable `partitionPinnedFirst(rows, pinned)` floats the pinned rows
to the top, **preserving the engine order within each partition** (Astra's
engine order is total with tie-breaks; it does not copy kylins's date
re-sort). The overlay applies in `MessageList` at the `shownRows` choke
point (after the filter, before the date headers), so it propagates to the
rendered items, the Shift-range, and Select All without touching the
snapshot, the engine order, the selection carry (key-based), or ShowMore
(which compares the raw row count). The search branch keeps its own order
(the overlay is folder/unified-list only, kylins's precedent).

## D2: the toggle and the indicator

A row-menu **Pin / Unpin** entry (label flips on the row's pinned state,
after "Open in New Window", batch-capable through the menu's forEach), and
a **persistent** pin indicator on the row (a `PinIcon` in the trailing icon
cluster beside the paperclip, never hover-only, with a "Pinned"
title/aria-label). The row subscribes to the pin store directly (no
prop-drilling through the virtualiser). `PinIcon` joins icons.tsx (the
Hugeicons Pin02Icon already in the installed package).

## D3: the contract

docs/list-selection.md binds the list surfaces; the slice adds a pin section
there (the per-account pin set, the stable partition that preserves engine
order, the folder-only overlay, the windowing honesty) and mirrors it to zh.
A new Mail-area capabilities row lands, with its zh mirror. The
porting-backlog "pin" entry resolves with the slice. A changelog fragment
lands.

## Known gaps recorded, not hidden

- Pins float only within the loaded snapshot window; a pin on a row beyond
  the visible limit is invisible until ShowMore loads it (kylins's identical
  limitation; no fetch).
- The overlay is folder/unified-list only; search results keep their order.
- A flat-mode message pin and a threaded-mode conversation pin are distinct
  identities; mapping between them needs thread ids on flat rows (engine
  work), a named follow-up. Pinning a thread row pins the whole conversation
  as displayed.
- No "Pinned" filter tab in v1 (cheap but deferred to keep the slice tight).
- Pins are device-local (localStorage), not synced across devices.

## Test-first

Every behaviour lands with its red. The store test: toggle/isPinned, the
per-account keying, the defensive parse (malformed drops), the localStorage
persistence. The partition test: pinned rows float first, engine order
preserved within each partition, non-mutating, an empty pin set is the
identity. The list test (a new MessageList.pin.test.tsx, the existing file
is near the cap): pinned rows render first with the indicator, the overlay
leaves selection and ShowMore intact, search mode is unaffected. The menu
test (a new rowMenu.pin.test.ts): the Pin/Unpin label flip, the toggle
writes the store, batch pins. Any mid-test store mutation under fake timers
rides act(). No live model proof (pure UI); the offline battery is the
proof.