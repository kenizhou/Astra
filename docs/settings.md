# Settings: the Astra Mail contract

**Scope.** How the settings screen is organised, where a setting is
persisted and who consumes it, and what the accounts page may and may not
do to a stored account. Binding on every surface that ships a settings
screen or an account editor. Ported in substance from mail-calendar's
`settings.md`, scoped to what Astra Mail ships today; the gaps are
recorded at the end, never left silent.

**Principle.** *A setting is one thing, decided once.* Its category is
decided once here, its value persists once, in one document, and every
consumer reads the same snapshot rather than keeping a copy of its own.
Two surfaces that disagree about a setting are two bugs, not a
preference.

## The categories that ship

The screen opens as a modal over the shell: a category source list
beside a detail panel. The categories follow mail-calendar's taxonomy in
name and order, and an absent category stays absent, never stubbed: a
row that renders nothing invites a support answer nobody can give.

| # | Category | What lives in it |
|---|---|---|
| 1 | **General** | Appearance (Follow host, Light, Dark) · Clock (24-hour on, 12-hour off) |
| 2 | **Calendar** | Display zone · First day of the week · Horizon · Default calendar |
| 3 | **Reading** | Conversation grouping (flat or threaded) |
| 4 | **Composing** | Quote style (the quoted original below or above the reply) |
| 5 | **Accounts** | Every configured account, each opening its own page |
| 6 | **About** | The version and the attributions, from the core, not the web's guess |

**Every control reflects the persisted truth, never local optimism.** A
change dispatches its intent and the control takes the value the
republished block carries, so a refused or failed write leaves the
control showing what stands and the sentence saying why. The screen
pulls the snapshot every time it opens, so the first paint is the
persisted state, not a stale store's memory of it.

## The persistence document

**One store: the preferences file beside the engine store.** Host app
preferences, not synced data, so they live in a small TOML document
(`preferences.toml`) in the app data directory rather than in the store
itself. Every set verb, whichever category it belongs to, writes the
field, persists the document, and only then answers: a changed setting
is on disk by the time the surface hears back, and the whole settings
family shares that one tail.

- **The document is additive.** Every field carries a default, so an old
  file written before a setting existed loads clean with the shipped
  default for the newcomer, and a field nobody has moved writes no entry
  at all. A missing, unreadable, or unparseable file yields defaults
  rather than an error: preferences are best-effort state, and a host
  that cannot read them starts from the defaults on the next boot.
- **The write is a read-modify-write against what is on disk right
  now**, so a concurrent write to an unrelated preference (a folder tree
  collapsed while the settings screen is open) is not clobbered by the
  in-memory copy.
- **A failed persist is honest about the split.** The in-memory copy
  moves either way, because that is the state the running app is in; the
  snapshot's error channel says the file did not take it, so a relaunch
  loses the change until a later write persists. The channel states the
  newest write only: a write that lands clears it.

## What the core consumes

The calendar is the settings block's biggest consumer, and every
consumption reads the persisted preference at the moment of use, so a
change moves the product with no restart and no code path of its own.

- **The display zone** is an IANA id the core validates on dispatch: a
  zone the engine's bundled tzdata cannot resolve is refused
  (`zone_unresolvable`), because every floating and all-day instant in
  the diary resolves through the display zone and a zone that cannot be
  localised is a diary that cannot be laid out. A change re-expands
  every account's stored events **over the full span the watermark
  covers**, never the visible window alone: the engine's expansion
  rewrites only the span asked for, so a re-expansion over the window
  would strand every other span the watermark claimed while it still
  claimed it, and a later range inside it would skip its own widening
  and render confidently empty. An account whose re-expansion refuses
  keeps old-zone instants: the zone stands (it persisted), and the
  account's row carries `refresh_error` until a later zone change
  retries it.
- **The first day of the week** is the anchor a repeat rule carries: a
  created rule that names no weekday anchors to the week boundary rather
  than to a day, so it carries the setting's day as its `WKST`. A rule
  that names its weekdays keeps the engine's own default anchor, because
  on a weekly rule with an interval above one the anchor decides which
  named days each interval holds. The safety is the **round-trip
  guard**: a stored rule is editable only when projecting it onto the
  editor's shape and rebuilding it under the setting's day reproduces it
  exactly, so a rule whose anchor a save would move (a `WKST` the
  setting does not name, an exclusion, a skip policy) refuses by coming
  back different, never by being named, and can never be quietly
  rewritten.
- **The horizon** is how many days the calendar's cold window covers:
  a setting may name seven to three hundred and sixty-five days (a
  window shorter than a week cannot hold the week a cold grid paints),
  and the read clamps into the same bounds, so a hand-edited file cannot
  ask the rebuild for a window the product would itself refuse. The
  first window a launch paints sits a week behind now, wide as the
  horizon says; widening beyond it moves in month-sized chunks against
  the watermark, because expansion re-derives every stored event.
- **The default calendar** is the settings block's, additively: the
  calendar snapshot's rows already carry the server's own default
  marking, and the app-level pin travels beside them in the settings
  block, the one source the create dialog reads. The two facts live on
  one surface, so they cannot disagree by living on two.
- **The grouping and the quote style** are the same discipline for their
  surfaces: the mailbox list's flat or threaded shape and the composer's
  quote order each read the one persisted field, and the composer quotes
  the original below the reply by default.

## What the host chrome consumes

- **The theme follows the host by default and keeps following it
  live.** Light and Dark are an explicit override; Follow host returns
  to tracking the desktop's own preference as it changes. The title
  bar's quick cycle and the General picker are two controls over the one
  persisted mode, so neither can strand the other.
- **The clock** drives how every timestamp the web view renders is
  spelled, 12-hour by default: one field, read wherever a time is
  formatted, so the mail list and the diary cannot disagree about what
  ten past noon looks like.

## The accounts page

The app is generic over its mail provider and cannot see the stored
configs: the OS secure store, the dial table, and the re-dial live with
the host. The settings surface drives them through **the account-info
seam**, a port the host installs at assembly. The read half answers one
info per configured account (its kind and its dav section's echo); the
rebuild joins that against the app's own account set, which stays the
authority on what is running. The address and the legs flags come from
the app's own facts (the identity, the providers' capabilities, the
installed contacts legs), so they are freshly true the moment a re-dial
installs anything.

- **An account with no host info renders the mail-only shape**: the
  kind, the address, no dav row. An info naming an account the app does
  not hold renders no row, because a config the app is not running is
  not an account the page can act on.
- **The masked echo.** The dav section's credential never crosses back
  over the seam: the echo carries the origins, the login, and
  `dav_password_set`, a boolean that answers whether a password is
  stored. The editor's password field therefore starts blank and says
  blank keeps; a typed password replaces the stored one verbatim.

### The dav editor's save

The save verb merges the editor's input over the stored section
host-side, dials the merged section, and persists the config, in that
order. **The merge rule**: each field the input states replaces what
stood; each field it leaves absent keeps it. Blank text states a cleared
origin (the config layer's own accessors read blank as no server); a
blank or absent username keeps the stored login, because a section with
no login dials nothing and the row always echoes the standing login
back. The password keeps whenever the input states none, because the
echo never carries it; a stated password replaces the stored one
verbatim. A merged section that names no origin at all is refused:
nothing is diallable, and a stored section with no origin would refuse
to load at the next boot.

**The dial runs before the persist.** The contract keeps everything that
stood on a failure, so a failed re-dial must leave the stored section
exactly as it was: the merged config dials first (through the real
`connect_dav_providers`, both legs independently), and only a dial that
answered legs is written to the secure store. The app then installs what
came back: the dialled calendar provider **replaces the account's
calendar leg wholesale** (the providers that carry no calendar
capability stay, the dialled leg appends after them), and the contacts
leg installs beside the mail providers. A busy account (a dispatch still
holding it across an engine call) is probed before the host's store
moves and the save refuses with nothing host-side changed.

**A failed write keeps everything that stood.** The failure lands on the
account's row as `dav_error`; nothing installed, stored, or torn down
moves. The old legs keep serving whatever they were serving.

**The clear** (`None`) drops the section and answers empty legs: the
app tears both legs down and forgets the store domains they filled (the
calendar and contacts domains), so the cleared account's diary and
people views render empty rather than haunting the engine with rows no
leg will ever refresh.

### Remove account

**Destructive, confirmed, and naming what goes.** The confirmation names
the address and says that its mail, calendar, and contacts go together
and that this cannot be undone. The host's halves go in order: the
credential entry (the durable half) deletes first, so a refusal answers
with nothing moved anywhere; the dial table's entry follows. The app
then tears its own halves down: the account entry, the contacts leg, the
write states, the open reading that pointed inside the account, the
row's standing sentences, and every store domain the account filled,
mail included. The account-scoped preferences (its expansion map, a
selection pointing inside it) go with it, and the selection returns to
the unified view, so a later re-add starts clean rather than inheriting
a shut tree nobody remembers shutting.

## The error channels

One typed word on the snapshot's `error` names why the newest total
write refused or failed; two per-account channels name the failures that
belong to an account rather than the block. All state the newest write,
never the history.

| Channel | Says | Where it renders |
|---|---|---|
| `persist_failed` | The write landed in memory but the preferences file did not take it | One sentence on the screen, following the newest write: every category renders it until the next write answers |
| `zone_unresolvable` | The typed id names no zone the engine can resolve; the standing zone keeps | Under the zone picker |
| `horizon_out_of_bounds` | The named horizon sits outside the seven to three hundred and sixty-five day bounds; the standing horizon keeps | Under the horizon field |
| `remove_failed` | The account's removal could not be completed; nothing went | On the account's page, under the confirmation |
| `dav_error` (per account) | Why the account's dav editor last refused or failed; the section that stands is the one the editor shows | On the account's row, under the editor |
| `refresh_error` (per account) | Why the account's last zone change could not rewrite its stored occurrences; the zone stands and a later change retries | On the account's row |

## What ships today

| Surface | Ships |
|---|---|
| Desktop | The settings modal from the title strip's gear: the six categories above, each control persisting before it answers; the accounts page with the dav editor and the confirmed removal; the theme and clock applying across the chrome; the zone, first day, horizon, default calendar, grouping, and quote style consumed by the diary, the create dialog, the list, and the composer |

The live proofs: the gated harness suite drives the seam end to end over
a scratch config document that mirrors the host store's merge rule and
its dial-before-persist ordering. One proof seeds an event and a card,
starts the app mail-only, saves a section naming both origins, and reads
both legs back through the product's own snapshots (the event on the
diary, the card on the people view, the masked echo with the password
flag and both legs flags on the row). A second saves a good section,
then one whose both origins name a port nothing serves: the dial fails,
the old legs keep rendering the seeded event, the row carries
`dav_error`, the stored section keeps the origin that stood, and a later
save in the editor's blank-keeps shape lands cleanly. A third clears the
section and reads both legs torn down, the store domains forgotten. A
fourth pins the now line's zone id republished by a zone change. Every
seeded artefact is withdrawn at the end.

## Known gaps

- **The categories that do not ship, by name**: the Allodia account,
  Signatures, Notifications, Privacy, Advanced, and Diagnostics are
  absent, and so are the entries inside shipped categories that
  mail-calendar carries: the app language and the default mail app
  (General), swipe actions (Reading), the default send account
  (Composing), and a reset. Each arrives with the slice that implements
  it, in the slot the taxonomy gives it.
- **The gear is a text button.** The title strip's entry reads Settings
  and draws no glyph; the icon set arrives with the chrome pass.
- **The work week's first day is an explicit picker, not a locale
  default.** Nothing follows the app locale, because the app ships no
  language setting yet; the weekday labels read English until then.
- **The per-message quote override has no web control.** The app default
  ships in Composing; mail-calendar's per-message opt-in is not offered
  in the composer.
- **The horizon's out-of-bounds sentence is bounded client-side
  first.** The typed field refuses a commit outside the bounds before
  the core ever sees it, so the core's own typed refusal is the belt
  behind the client, not the path a user meets.
- **The zone picker offers the browser's own list**, so an engine that
  supports a zone the browser does not list can be set by a hand-edited
  file but not through the screen.
- **No device-zone prompt.** A device whose zone changes is not offered
  the change; time-zone-aware display with the prompt is the ported row
  that stays open.
