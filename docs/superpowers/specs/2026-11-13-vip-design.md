# VIP senders design

**Status.** Decided 2026-11-13. Slices branch from main at this commit.

**Scope.** There is no way to mark a sender as important and see their mail at a
glance. This kylins addition adds VIP senders: mark a sender VIP, see a badge on
their rows, and filter the list to VIP mail. It is the pin/quick-steps web shape
(zustand + localStorage, pure view state, no intent) keyed by sender address.
Pure client/web: no engine, no host, no viewmodel, no new wire type. Device-local.
No eval/pipeline bump.

## D1: the VIP sender store

A new client/web/src/features/vip/vipSendersStore.ts (cloning pinnedRowsStore.ts):
a zustand store over localStorage key astra.vipSenders, a Set of bare sender
addresses (lowercased, trimmed; cross-account, unlike pin's per-account rowKeyId),
defensive parse (a malformed payload drops to empty, never thrown), storage
failure logged never thrown. toggle(address) / add / remove / has, and the set.
The address is the bare addr-spec (the row's sender_address), normalised so
"Ada <ada@x.com>" and "ada@x.com" agree. An empty/blank address is refused.

## D2: the filter tab + the row badge + the toggle

The list filter gains a 'vip' tab: listFilter.ts ListFilter extends with 'vip',
filteredRows answers the rows whose sender_address is in the VIP set (the set is
read at the list's render choke point, the pin pattern; the filter stays
client-side, never an intent), filterCounts adds the vip count. The tab renders
in ListToolbar.tsx beside All/Unread/Flagged/Attachments. A VIP badge renders on
MessageRow.tsx for rows from a VIP sender (a small marker, the existing badge
idiom). The row menu gains an Add/Remove VIP batch toggle (cloning pinItem: pure
view state over the local set, no intent, the label flips on the targets' state,
the batch lands one state). No ipc.ts change (at the cap); the feature owns its
types.

## D3: the contract

docs/list-selection.md gains a VIP senders section (the sender-keyed set, the
cross-account scope, the badge, the filter tab, the batch toggle, the
device-local persistence). docs/capabilities.md gains a row. A changelog fragment
lands. docs/porting-backlog.md moves VIP into the shipped list. The zh mirrors
move in the same commit.

## Known gaps recorded, not hidden

- Device-local: localStorage, no cross-device sync; the VIP set is per-browser-
  profile, not per-account (a sender marked VIP is VIP across accounts).
- The badge and filter read the loaded rows only (the list filter is client-side
  over the snapshot, like the other tabs); a VIP sender with no loaded row shows
  nothing until their mail loads.
- No notification or priority routing for VIP mail (a follow-up); this slice is
  the marker, badge, and filter.

## Test-first

Every behaviour lands with its red. Store tests (parse drops malformed, the
address normalisation, toggle/add/remove/has, blank refusal, persist failure
logged not thrown) mirroring pinnedRowsStore.test.ts. Filter tests (the vip tab
answers the VIP senders' rows, the count, the empty set) extending
listFilter.test.ts. A row-menu test (the batch toggle lands one state, the label
flips) mirroring the pin toggle test. A MessageRow badge test (the badge stands
for a VIP sender, absent otherwise). The vitest traps apply: wrap mid-test store
mutations in act(), mock outer functions whole. The offline battery is the proof
(a pure web feature; no live harness proof).