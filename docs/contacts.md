# Contacts: the Astra Mail contract

**Scope.** What the People view shows, how one person is told apart from the
cards that were filed as them, what a contact write may and may not do, where
the composer's recipient completion draws its answers from, and how the
contacts leg of an account is dialled. Binding on every surface that ships a
people view or a recipient field. Ported in substance from mail-calendar's
`contacts.md`, scoped to what Astra Mail ships today; the gaps are recorded at
the end, never left silent.

**Principle.** *An address book is a list of cards, not a list of people.* The
engine decides who is one person; every surface renders that decision and
discloses it, and a write never crosses the boundary between them.

## The engine decides who is one person

The same person is routinely filed in several books across several accounts,
so the engine derives a unified person from the cards, and every surface
renders people, never cards. The join is deliberately narrow:

| Joins on | Never joins on |
|---|---|
| a shared canonical email address | a name |
| | a phone number |
| | an organisation |

**Names never join, and this is the load-bearing half of the rule.** Two
different people commonly share a name, and a merge is destructive to the
user's understanding: they filed two colleagues and now see one, with one set
of details silently winning. A missed merge is a cosmetic annoyance; a wrong
merge hides a person. The asymmetry is why the join is conservative rather
than clever, and why no surface may add matching of its own on top.

### Nothing is merged without saying so

- The list row shows **In N accounts** whenever the person came from more
  than one account. The count is distinct accounts across the person's
  sources, counted once in the snapshot; a host never recounts, and a person
  filed in two books of one account still reads as one account.
- **Never show In 1 accounts.** It is noise on every ordinary contact, and
  ungrammatical noise; the count crosses no key until there is a merge to
  disclose.
- The detail names the sources under **Also in**, and tags each individual
  value with the cards that supplied it: this address is in your work
  account, that one in both.

### A retired person id still resolves

A merge retires ids, and the engine keeps the retired ones pointing at the
surviving person, so a row a surface is still holding after a background
refresh merged it still opens. `OpenPerson` with an id the list minted never
needs the list refreshed first, and a detail that resolves to nobody means
the person is genuinely gone, never merely renumbered.

## The fold and the monogram: one derivation in the rebuild

The engine's page arrives ordered by the raw code points of the lowercased
display string, and the engine at the pinned revision carries no section or
avatar model. The presentation facts are therefore derived **once, in the
app's rebuild**, and every surface shares them: this is the recorded
deviation from the ported contract, which puts the same derivations in the
core of mail-calendar.

- **The section fold.** One alphabet, with an accented letter filed under its
  base (Émile under E, between Emil and Emma; Øystein under O), and one `#`
  bucket after it for everything that does not start with a Latin letter:
  digits, symbols, and every other script. The fold covers Latin-1 Supplement
  and Latin Extended-A, the diacritics a European address book holds; folding
  a Cyrillic or CJK name to a Latin letter it is not would be worse than
  saying nothing.
- **The fold orders the list, not the page.** The rebuild re-orders the
  page's rows by its own folded key before publishing, because the engine's
  order and the fold's sections would otherwise disagree: raw code points
  sort Émile after Emma, and a sectioned list that lays rows out in page
  order shows E holding a row past its letters. The rows are never
  re-filtered: the engine matched the query over every person, and a
  host-side filter over the page would silently unfind anyone the page cap
  had already cut off.
- **The monogram.** The initials are the first character of the first and
  last words of the name, uppercased and otherwise untouched (Émile keeps
  his É); the address's first character when the sources carry no name;
  empty when they carry neither, the signal for the host to draw its own
  person glyph. The fill comes from a pinned palette of eight colours, keyed
  on the lowercased primary address so it is stable per person, with the
  person id as the fallback key: a person cannot change colour between
  surfaces, and two people who share a colour still differ in initials.

The same derivation stands behind the snapshot's rows, the detail, and the
host's avatar command, so the same person cannot be `AL` on one screen and
`A` on another.

### A name or the signal, never a placeholder

A card may legitimately carry an address and no name. The row's display name
is **empty** for such a person, never a placeholder: the host supplies the
words in the user's language, and the fold and the monogram still key on the
identity the engine chose (the address). A placeholder invented in the core
could only ever be English.

## A write names a card, never a person

Contacts can be created and edited. Deleting them is not offered (a gap
below). Everything else on the People view is about people, and a write is
not: the engine joins several accounts' cards into one person, so saving the
values on screen without choosing one of those cards would file the work
account's details in the personal account's book, and the result would look
perfectly right.

- **An edit carries the person, the account, and the card together.** The
  person rides so a row held across a merge still resolves; the account and
  the card are what the write actually goes to.
- **The editor is seeded from the card**, through the engine's stored-card
  read, never from the merged detail the user was just looking at. A merged
  person's addresses belong to different cards, and offering them all for
  saving into one is the same mistake in another form.
- **The surface asks which card when there is more than one**, naming the
  account. One card opens straight into the form.

### Both affordances are conditional, and each answers a different question

| Affordance | Shown when | Because |
|---|---|---|
| Create | the snapshot's create targets are non-empty | A target is a writable address book; a directory or a read-only shared book is a place contacts come from, and offering one produces a save that fails on the server after the user has typed everything in |
| Edit | the detail's editable cards are non-empty | The same rule, per person |

A person with no editable card says so, in as many words, rather than leaving
it to be inferred from an absence or a disabled pencil the user presses twice
and then doubts.

### What an edit must not destroy

A card holds far more than the editor shows, and an edit leaves all of it
alone:

- **A patch names only the fields that changed.** Every field an edit names
  is replaced, and a replacement built from a form loses whatever the form
  cannot show: an address's context, an organisation's units, a property's
  own identifiers. A changed field is rebuilt **on top of** the property the
  card already carries, so the metadata rides untouched, and an unchanged
  field is left out entirely. An edit that changed nothing sends nothing.
- **A structured name is written only when there is a surname.** A card may
  legitimately carry a formatted name and no structured one, and the editor
  seeds such a card by putting the whole name into the given-name field.
  Emitting components from that would file Ada Lovelace as a first name with
  no surname, in every other client the user owns, on a save that changed
  nothing.

The order of the addresses is the order on the card, because the first one is
the person's primary address: what the avatar's colour and the list row key
on.

### A card must be findable again

A create is refused unless it names something to file the card under: a name,
an organisation, or an address. The formatted name is derived from the first
of those that is present, so a company contact reads as its company rather
than as a blank row.

### What the status means

- **`FailedWithRetry` does not mean rejected.** A write whose server call
  succeeded but whose post-write reconcile did not settle has **landed on the
  server**; only the local copy is briefly stale, and the re-read the next
  refresh performs heals it. The surface tells the user the save could not be
  confirmed and that a refresh shows what stood.
- **`Invalid` is a different sentence.** The core refused the form before
  sending anything, so there is something to correct and retrying unchanged
  would be refused the same way. A surface states it under the form the user
  is still looking at, and nowhere else.
- **`Writing` and `FailedWithRetry` render beside the list**, never inside
  the detail pane: a create is made with nobody open, so a line drawn there
  is one the user who just created a contact cannot read.

## Autosuggest

Composer recipient autosuggest draws on the same people index, but it is not
gated on contacts: the engine mines recipient history from each account's
Sent mail during the ordinary sync, so completion works on an account with no
address book at all, which is most accounts. A suggestion that came only from
sent mail is as valid as one from a saved card; the snapshot marks the two
apart and never hides the history-only ones, which are usually the most
useful.

**The token, not the field.** The composer hands the read the text after the
last comma in the field, never the field: the field holds a list in one
string, and querying it whole finds nothing once a first recipient is
entered. A blank token returns nothing; a dropdown of everyone the user has
ever mailed, the moment the field takes focus, is noise rather than help.

**Addresses are inserted bare**, not as `Name <address>`: the engine parses
addresses, a display name adds nothing it uses, and a name containing a comma
would split into two invalid recipients the user would not discover until the
send failed.

**Finished recipients are pills; the one being typed stays text.** What the
user sees as one recipient and what the completion treats as one recipient
cannot disagree, and a wall of comma-separated addresses offers nothing to
tap to remove a wrong one. The field's value stays **one comma-separated
string**: pills are a view of it, never a second source of truth, so nothing
can be on screen that would not be sent. Each pill carries its own remove
control.

**A pre-filled field has nothing in progress.** Every address a caller
supplies is finished, so the field is normalised to that at the moment it is
seeded and each address renders as its own pill: a reply's derived
recipients, left raw, drew one pill beside a loose address and looked like it
had dropped the people it was holding. Normalise the seed, not the field a
frame later (a rewrite after opening reads as an edit), and compare against
the normalised opening value when asking whether anything was typed.

**The caret goes to the end after any programmatic change**, so the next
keystroke never lands inside the address just inserted, and the composer
opens with the caret in **To** on a new message and in the **body** when it
is already addressed: one predicate, not two flags that can disagree.

**The list floats; it never displaces what follows.** It takes no layout
space, so Subject, the editor, and Send do not jump while the user types the
first recipient; it is opaque, it takes no focus (the keystrokes it completes
keep arriving), only the focused field's list is on screen, and it clears
the editor, which is a web view a toolkit will happily composite above its
own drawing.

**The ranking is the engine's**, passed through untouched: an exact address,
then a prefix, then a substring, each tier settled by saved-ness, recency,
and counts over sent-mail history beside the cards. A surface that re-ranked
the list would seed its own undo.

## Avatars

A person's avatar is the monogram above, derived once, until a photo arrives.
The photo is read from the engine's **cached** contact-photo store (filling
the cache is a background pass's business, so a screenful of strangers costs
no provider round trips), and it reaches the web view only through the host's
validated photo command, which serves a data URI for paths inside the
engine's own contact-photo area: the web view never reads the file system,
and no photo path crosses as a URL a browser could be pointed elsewhere by.

## The DAV legs

An account's contacts (and calendar) arrive through the optional `[dav]`
section of its stored config, which both legs dial:

- **The section** names the CalDAV origin and, when it stands apart, the
  CardDAV origin, with the login both legs present. The CardDAV leg falls
  back to the CalDAV origin when the section gives it none of its own,
  because nearly every server that speaks one DAV dialect speaks both at the
  same origin. A section naming neither origin refuses at load: a stored
  account that silently lost its origins is a bug to surface, not a contacts
  list that quietly never syncs.
- **The boot dial** runs both legs through the real `connect_dav_providers`
  beside the mail dial: the CalDAV provider is appended after the account's
  mail providers (the calendar paths pick it by capability), and the CardDAV
  leg is installed through `install_contacts` beside the account. The legs
  dial independently, and a dial that fails costs only its own leg, logged:
  the account keeps its mail half and the boot keeps going.
- Discovery is the engine's: the adapters resolve the well-known paths and
  the homes behind them inside their own connect.

Both legs are proven live against the harness (below).

## Privacy

Contacts are the most identifying data the app holds, and two inherited rules
carry them:

- **Never log content.** Log lines carry counts and ids, never a name,
  address, phone number, or organisation. Address-book ids are not content
  and may be logged; the cards in them may not.
- **Nothing new is sent anywhere.** Contacts sync to and from the user's own
  configured accounts and nowhere else; nothing about them enters any other
  payload.

## What ships today

| Surface | Ships |
|---|---|
| Desktop | People view beside Mail and Calendar: the sectioned list with monograms and the In N accounts disclosure, the detail with Also in and per-value provenance, the editor with its card choice and conditional affordances, composer autosuggest with pills, avatars through the photo command |

The live proofs: the gated harness suite runs the real `connect_dav_providers`
over a dav section naming both origins and the boot-shaped assembly over it
(appended calendar leg, installed contacts leg), seeds cards onto each
account's default book through the raw CardDAV face, and reads them back
through the product's own sync and snapshot: the fold order, the section
letters, the monogram, the one joined row that says In 2 accounts, and the
detail that names both sources. A second suite round-trips the writes: a
create into the writable book and a rename on the named card, each read back
off the server, every write state settling. A third submits one fresh mail
through the engine's send path and asserts the autosuggest ranking over this
run's own mined history. Every seeded card and every sent mail is withdrawn
at the end, so the harness keeps the shape the mail suites pin.

## Known gaps

- **Contacts cannot be deleted.** The engine carries the verb; the product
  does not offer it, because a delete is the one contacts write a user
  cannot undo and it wants a confirmation and an account-naming step of its
  own, on the terms the calendar's delete already has. The live proofs
  withdraw their seeded cards through the raw seeding helper, honestly
  marked as seeding-side cleanup.
- **Offline writes are not durable.** A contact write that fails stays
  failed rather than queueing: the same shortfall the calendar writes
  carry, recorded there.
- **Colleague directories need the OAuth account legs.** A Microsoft or
  Google directory is a contacts source the engine can read, but Astra Mail
  dials no OAuth accounts yet, so no directory stands behind the people
  index today.
- **JMAP contacts are unproven.** The engine's JMAP contacts adapter exists;
  no Astra surface or live proof exercises it, so the CardDAV leg is the one
  proven path.
- **The wizard has no dav field.** A stored account's `[dav]` section loads
  and dials at boot, but no surface writes one yet; the field lands with the
  settings slice.
- **The photo is re-read per mount.** The avatar command answers a data URI
  built from the cached file on every call, with no in-memory caching in the
  host, so a list of photo-bearing people costs a file read per avatar per
  render.
- **The mail list row carries no avatar.** Monograms and photos render on
  the People view and the composer's suggestions; the message list's rows
  draw none yet.
- **The list is capped at 200 people** with no paging surface: search
  narrows in the engine, so a person beyond the cap is findable but not
  scrollable to.
