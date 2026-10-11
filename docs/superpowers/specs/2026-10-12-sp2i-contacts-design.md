# SP2i Contacts Design

Date: 2026-10-12
Status: proposed
Parent spec: `docs/superpowers/specs/2026-08-...-workflow-design` (sections 5, 15)
Port contracts: mail-calendar `docs/contacts.md` in substance; the avatar
rules from its `avatars.md` references; the pre-spec survey at
`.superpowers/sdd/2026-10-12-sp2i-contacts/survey.md`.

## 1. Purpose

Contacts as one product surface: the unified people list with the merge
disclosure, the person detail with per-value provenance, contact create and
edit into a chosen writable card, composer recipient autosuggest over the same
index, avatars, and the account-model DAV extension that finally dials the
calendar and contact legs at boot (closing the calendar contract's first
recorded gap).

## 2. Decisions

- **D1 (surface)**: a `Surface::People` carrying the `PeopleSnapshot`: the
  unified rows (display name, the core's monogram letters and colour, the
  account-count disclosure, the section letter, the primary address), the
  open person's detail (source cards named by account, per-value provenance,
  the editable-card set), the write states, and the create targets (the
  writable address books). The shell's view switch gains People beside Mail
  and Calendar.
- **D2 (intents)**: the family `People(PeopleIntent)` with `Refresh`,
  `Search { query }` (matched in the engine, never re-filtered rows), `OpenPerson`,
  `ClosePerson`, `CreateContact { book, draft }`, `UpdateContact { card,
  patch }`. No delete: the ported contract does not offer it (recorded gap).
- **D3 (the join and its disclosure)**: the engine decides who is one person
  (the narrow join on a shared canonical address; never a name, phone, or
  organisation) and the client renders people, never cards. A merged row
  discloses `In N accounts` (distinct accounts, core-counted, never shown for
  one); the detail names the sources under Also in. A retired person id still
  resolves after a background merge. Ordering, matching, the section letter
  (an accented letter is a letter; `#` otherwise), and the monogram
  initials are the engine's.
- **D4 (writes name a card)**: an edit carries `(person, account, card)` and
  the editor seeds from that card, never the merged detail; the client asks
  which card when there is more than one. Create shows only when a writable
  target exists; edit only when an editable card does; a person with neither
  says so in words. A patch names only changed fields, rebuilt on top of the
  property the card carries; a structured name is written only with a
  surname; a create must be findable (a name, organisation, or address).
  Write statuses follow the calendar's shape plus `Invalid` (a different
  sentence, under the form); `Failed` means unconfirmed, not rejected, and
  the status line sits beside the list.
- **D5 (autosuggest)**: the composer's recipient fields complete the token
  after the last comma, insert bare addresses, query the engine's ranked
  suggestions (cards plus sent-mail history; history-only entries are never
  hidden; a blank query returns nothing), render finished recipients as pills
  with per-recipient removal while the field stays one comma-separated
  string, and float the list over the form (opaque, no focus, only the
  focused field's list, cleared above the editor). Seeded fields normalise
  their opening value and compare against it for dirtiness; the caret opens
  in To unless the composer is already addressed, then in the body.
- **D6 (the DAV account extension)**: `AccountConfig` grows an optional `dav`
  section (origin and credentials; the CardDAV leg falls back to the CalDAV
  origin per the ported rule), the account layer dials the CalDAV and
  CardDAV providers beside the mail ones, and the desktop boot installs every
  leg it dials. The calendar and the contact legs land together: one
  account-model extension, two gaps closed.
- **D7 (avatars)**: the monogram's letters and colour are the engine's, so
  the same person cannot differ between surfaces; a photo arrives through the
  engine's cached contact-photo fetch, one resolution shared by every
  surface that shows the person (the contacts row and the reading header in
  this slice; the mail row follows with the list polish).

## 3. Testing

- Unit (astra-app over the engine fakes): the snapshot rebuild (the join
  disclosure facts, the section folding, the ordering tiebreak); the write
  paths (the card-naming discipline, the patch-only-changed rule, the
  surname rule, the findable-again refusal, the status mapping); the
  autosuggest token math (as pure functions).
- Wire: the PeopleIntent family + PeopleSnapshot literals pinned, both
  languages.
- Frontend (vitest): the list with sections and the disclosure; the detail
  with Also-in and provenance; the editor's card choice and conditional
  affordances; the pills, the floating list, the caret rules (jsdom-honest).
- Live (gated, harness): Stalwart's CardDAV on 2080 serves address books and
  cards; a sync read, a create into a writable book, an edit round trip, the
  joined person filed across both harness accounts (the `In 2 accounts`
  disclosure live), and autosuggest over the sent-mail history the mail
  suites already seed.

## 4. Known gaps (pre-recorded)

- No contact delete (the ported contract defers it with the calendar writes'
  offline durability).
- Colleague directories (Graph users, the Workspace directory) need the OAuth
  account slices; not here.
- The JMAP contact adapter ships in the engine but is unproven against the
  harness's JMAP face in this slice; the CalDAV/CardDAV legs are the proven
  pair.
- Photos on the mail list row follow the list polish slice.
