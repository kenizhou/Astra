# Search: the Astra Mail contract

**Scope.** What a mail search covers, in what order it shows what it found, how
the query language splits, and how the client offers to narrow it. Binding on
every surface that ships a search field or renders its results. Ported in
substance from mail-calendar's `search.md`, scoped to what Astra Mail ships
today; every divergence is recorded in the rule that owns it and in Known gaps,
never left silent.

**Principle.** Search answers *"where is that message"*. A person looking for a
message knows roughly **when** it arrived and rarely which folder it ended up
in, so search looks **everywhere by default** and shows what it finds **newest
first**.

## The rules

| # | Rule | Why |
|---|---|---|
| 1 | **Results are ordered newest first, never by relevance.** The tie-break is `(account, provider key)`, so the row sequence is total and identical across rebuilds. | A relevance order interleaves this morning's mail with a three-year-old thread and reads as no order at all. The engine's ranking still decides *which* hits are candidates; it does not decide the display order. |
| 2 | **The default scope is every account and every folder, except Trash.** The wire word names the exclusion: `all_except_trash` (mail-calendar's variant is `AllFolders`, which overstates). | A message the user threw away is not what they are looking for, and a deleted copy sitting beside the live one is noise. |
| 3 | **Trash stays reachable**: open it and narrow to the current folder (rule 4). The exclusion is a default, not a wall. | A folder you cannot search is a folder you cannot use. |
| 4 | The scope filter offers **three choices**: *all except trash*, *this folder*, or *this account*. *This folder* **mirrors the mailbox list exactly**: the selected folder, or (in the unified view) every account's Inbox. When the list shows one account's own all-mail view, "this folder" is that whole mailbox, Trash included, and **converges with *this account***. *This account* is Astra's own third choice, beyond mail-calendar's two-way "this scope or all mail" filter: whosever mailbox the list is showing, all of it, Trash included; in the unified view, every account's mailbox. | The narrowing has to mean what the user sees: in the unified all-inboxes view "this folder" is the set of inboxes on screen, not any one folder. An account's own all-mail view names no single folder, and pretending it does would hide mail on a technicality, so the two readings converge. |
| 5 | **Leaving search restores the view it was opened from**: the same account and folder, unsearched. Escape in the field, blanking the query, and Exit search all clear, and nothing else moves. | Search is a mode over the list, not a destination. |
| 6 | **Clearing the query resets the scope** to the default, in the app and in the field's dropdown, as one action. | A narrowing the user can no longer see is a narrowing they will not think of: the next search would silently be smaller than it looks. |
| 7 | The scope is **session state, never persisted**, and a narrowing with no live query **is refused outright**: nothing is re-run, nothing stored. | It is a filter on one search, not a preference, and there is nothing to narrow without a query. Tighter than mail-calendar's core, which stores the scope whatever the list is doing. |
| 8 | **A search states how far back it looked**, on the results surface, and the note is the route to the depth: opening it lands on the settings' accounts section, where each account's depth is set. The horizon is the **narrowest** synced window among the accounts the scope covered, in whole months, carried as `depth_months`: a plain count, never words on the wire. A scope that searched **no** account folds to zero and claims no horizon at all, so no note stands to be opened. | Search reads the local store and nothing else, so it finds only what sync depth kept. Unqualified, an empty result claims *"there is no such message"* when it means *"not in the last three months"*; only the second is something the user can fix, and the note says where. One three-month account makes the whole answer three months old at best, so the narrowest wins. |
| 9 | **Results follow the list's grouping.** With conversations on, a match is shown as the conversation it belongs to: the row carries the **whole** thread, is labelled by the newest message that **matched**, and sorts on that message (rule 1, measured against the query). A conversation nobody's query touched is never listed. | Search is a mode over the list, not a second kind of list: a row that stands for its whole thread in the mailbox has to stand for its whole thread here. Labelling and sorting on the match keeps the answer legible: a row summarised by an unrelated later reply reads as the wrong result. |
| 10 | **What is on screen answers the query as it now stands.** A search overtaken while it ran is discarded, never published; and the field is quiet for **500 ms** before the app is asked at all. Escape and Exit search act at once, being navigation rather than a query; a field blanked by backspace crosses on the same 500 ms pause as a typed query. | Searches run concurrently and finish out of order, by wide margins: a two-letter prefix matches half the mailbox and takes longer than the finished query. Published unconditionally, the screen goes to whichever finished last, which is reliably the broadest and least useful one, and nothing corrects it until an unrelated rebuild lands. |

The horizon (rule 8) is `SearchState::depth_months`, absent with the whole
search state for every list nobody searched, so a surface keys the whole line
off one field. The app sends a count and never words: the sentence is assembled
in the surface (`Searched the last 3 months`, singular `month` at one).

The ordering (rule 1) and the scope semantics (rules 2 to 4 and 6) live in the
app: `astra-app/src/search.rs` decides which accounts and folders answer, the
projection `list_rows::search_project` in `astra-app/src/list_rows.rs` orders,
and `Intent::SetSearchScope` carries the filter. Rule 9 rides the same
projection the mailbox list uses, with the hits' conversation members marked
out of scope, so grouping, labelling, and ordering fall out rather than being
re-derived. Rule 10 is split between the two layers, because each half can
only be enforced where it is: the app discards a superseded list (the
generation guard every list rebuild rides, re-checked before publish), and the
client debounces its own field (`client/web/src/components/layout/TitleBar.tsx`),
since only the client knows a keystroke happened. Neither half substitutes for
the other.

## The query language

The box takes a small DSL: `from:` and `to:` prefix filters (the keyword
case-insensitive, the value verbatim), quoted values that keep their spaces,
and a free-text remainder. `monitor from:ada@example.test` and
`from:ada@example.test monitor` mean the same search. A colon inside a quoted
span does not make an operator (`"a:b"` is a phrase), and an unknown prefix
(`fromm:x`, or the engine's own `subject:lunch`) stays free text: the engine's
wider vocabulary is the engine's to parse.

Matching belongs to the engine. The app splits the two product operators out
of the raw string, then hands them back **inside the engine's own query
string** (the engine's search surface accepts them there), so no app-side
filtering of the from:/to: operators runs (the scope's folder filter is
app-side): the filter narrows inside the match itself, exactly as the same
query typed raw would. The split exists to be forgiving where the engine is
strict:

- **A prefix with no value is dropped, not refused.** The engine refuses
  `from:` outright and would kill the whole query; the split drops the token
  and rebuilds the engine query from the survivors, so a mid-typing `from:`
  searches the remainder.
- **A query that rebuilds to blank answers no rows, never the whole
  mailbox.** A query of nothing but malformed prefixes names nobody; the
  honest answer is empty.

Two corners of the split diverge from the engine's own parser and are owned in
Known gaps: a repeated prefix keeps the later value, and a value mixing inner
quotes with spaces is re-wrapped on its way back.

## Candidates vs. results

The engine ranks each account's hits by **relevance** and caps them (500 per
account). The app treats that as a **candidate set**: it applies the scope's
folder filter, merges every account's survivors, orders them by date, and shows
the newest 100. Search results have **no show more**: the count header states
the rows shown, and a query matching thousands shows only the newest hundred
and says nothing about the rest. The results are a capped set, never a windowed
one; the list's windowed paging does not ride them.

Two consequences worth knowing, both in Known gaps: a very broad query can push
a *recent* match out of an account's top-500 by relevance, and a query matching
more than 100 messages shows only the newest 100.

## Typing does not mean searching

A search is a full-text query per account plus a store read per hit. The client
therefore **debounces** the query, 500 ms after typing stops (rule 10), rather
than dispatching per keystroke; Escape and Exit search stay immediate, and a
field blanked by backspace crosses on the same pause.
Mail-calendar measured the cost of per-keystroke dispatch on a five-account
device: the same query ran ~2.0 to 2.5 s per stacked rebuild against ~0.86 s
debounced. The delay buys latency back; it does not decide what is shown. Which
of the searches still in flight may reach the screen is rule 10's other half,
and it is settled in the app.

## Where the field lives

Rules 1 to 9 are about what a search *answers*, and none of them moves with the
box it is typed into. What the placement has to respect is rule 2: the default
scope is **every account and every folder**, so a field that sits over the
message list is a control whose reach is wider than the column it is drawn on.

Astra Mail ships one surface today: the web UI inside the desktop shell. There
the field holds the **title bar's centre**, between the two drag regions, with
the scope dropdown riding inside it. That is Outlook's own placement, and the
only surface whose centre is not a pane's. Further surfaces take the field's
contract from this document, not from this paragraph.

## The surface matrix

| Rule | The shipped surface (web UI, desktop shell) |
|---|---|
| 1 Newest-first order | yes |
| 2 Trash excluded by default | yes |
| 3 Trash reachable by narrowing | yes |
| 4 Scope filter | yes, `SetSearchScope`, three choices |
| 5 Exit restores the view | yes: Escape, blank, Exit search |
| 6 Clear resets the scope | yes |
| 7 Session state, no queryless narrowing | yes |
| 8 Horizon stated | yes, the depth note, and it opens the settings on the accounts section where each account's depth is set |
| 9 Grouping obeyed | yes |
| 10 Debounced, superseded discarded | yes, at 500 ms, not mail-calendar's 250 ms |

## Known gaps

- **The depth note's link opens the accounts section, not the specific
  account's depth control.** The note opens the settings on the accounts
  section, and each account's own page there carries its depth; the settings
  screen offers no deeper link into one account's page. The per-account depth
  itself ships (settings.md), and it and the horizon read one value, so the
  note and the setting it reaches can never disagree.
- **The horizon is a month count, never an unbounded shape.** Mail-calendar's
  core sends `AllTime` or a month count; Astra's state carries
  `depth_months: u32`, a plain count, and no search ever looked at all time:
  every account syncs a depthed window today. The divergence is honest while
  it lasts; an unbounded depth will need the shape to grow.
- **The debounce is 500 ms, not mail-calendar's 250 ms.** Recorded in rule 10
  and owned here: the longer pause trades a quarter second of feel for fewer
  overlapping runs against an engine whose searches cannot be cancelled
  mid-query.
- **Selecting a grouped result row selects it locally, without opening the
  reader.** With conversations on, clicking a thread row highlights it and the
  conversation does not expand; mail-calendar's clients expand the thread in
  place. Astra's conversation expansion arrives with the thread slice. A flat
  result opens the reading pane as usual.
- **A repeated prefix keeps the later value.** `from:a@example.test
  from:b@example.test monitor` searches from `b` alone; the engine's own DSL
  would accumulate both as alternatives. The split's seam carries one filter
  per field.
- **A quoted value mixing inner quotes with spaces is re-wrapped.**
  `from:a"b c"d` survives the split verbatim, but the rebuild re-quotes values
  that hold whitespace, so the engine parses `from:"a"b c"d"`, not the token as
  typed. A plain quoted value (`from:"ada lovelace"`) round-trips exactly.
- **The horizon states the sync-depth policy, not what has finished
  downloading.** While an account is still backfilling, the note names what the
  app is *for*, not what it *holds*. The engine reports every searched scope as
  complete unconditionally, so there is nothing more honest to read; the
  progress surface says a sync is running in the meantime.
- **A broad query can miss a recent match.** Ordering by date over a set the
  engine truncated by *relevance* means a message that ranked below an
  account's 500th-best can be dropped even though it is newer than results that
  are shown. The real fix is a date-ordered query in the engine (its
  `search_mail` ranks by relevance only), tracked upstream as engine issue #83,
  which also covers searching several accounts in one query instead of the
  per-account loop the app runs today.
- **No pagination.** Results stop at 100 rows with no show more, so a query
  matching thousands shows only the newest hundred and says nothing about the
  rest.
- **Junk is searched by default.** Deliberate (rule 2 names Trash only,
  matching Outlook), but worth revisiting if it proves noisy: Gmail excludes
  both.
