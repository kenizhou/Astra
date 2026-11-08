# Timestamps and language: the Astra Mail contract

**Scope.** How Astra Mail spells dates and times: the language
mechanism that drives the words, the calendar's own display zone, the
device-zone offer, and the message timestamps' display zone. Binding
on every surface that renders a date, a time, or a weekday label.

**Principle.** *The words follow the language; the hours follow the
zone; and the two never touch.* A language change moves the words
alone, and a zone change moves the hours alone, so a diary in French
over Tokyo hours is one setting each, never one setting guessing the
other.

## The language mechanism

The settings block carries one language tag, `en-GB` when nobody has
moved it. The tag names the mechanism, not a catalogue of
translations: the picker offers exactly the catalogues that ship (one
today, English), the core holds no language list of its own, and the
honesty sentence under the picker states what follows it: *Dates and
the first localised surfaces follow it today.* An absent or empty tag
keeps the default.

## The dates following

The date formatters read the standing tag on every format, so a change
moves the product with no restart:

- **The weekday and month words.** The calendar's day labels (the full
  day, the agenda header, the month title, the grid's short weekday)
  and the mail list's sent-at stamps spell their month and weekday
  words in the standing language, derived over each instant's own UTC
  identity so a label never shifts with the host's clock.
- **The en-GB spelling.** The list's short month follows the locale's
  own catalogue rather than a hand-kept list, so en-GB spells
  September's short form `Sept` (the catalogue's own word), a visible
  change from the hand-kept `Sep` the list carried before the tag
  stood behind it.
- **What stays English.** The list's group words (Today, Yesterday,
  Earlier) and every surface outside the first localised set stay
  English until their catalogue lands; the full app copy is the named
  remainder the localisation row carries.

## The calendar's zone

The calendar's display zone is its own setting, untouched by the
language tag: the labels derive over UTC instants whatever the
language stands at, and the diary's hours resolve through the zone
picker's own write. The clock's shape (24-hour, or 12-hour with a
meridiem) is the clock setting's own, shared by the list and the
diary so one time spells one way everywhere.

## The device-zone offer

When no zone has been picked and the default UTC stands, the diary
offers the device's own zone:

- **The offer is once per zone value per process.** A value that
  returns (a flapping device zone) was already offered and does not
  nag again; a genuinely new value offers.
- **The first-run latch.** Once any offer has ever been made, a later
  process's first report never offers again, so a host's repeated boot
  reports never nag either.
- **A picked zone stands, and the offer is not even made.** The pick
  is its own persisted marker, set by the zone picker's write and
  never by the offer's acceptance, because the standing zone value
  cannot tell an explicit UTC pick from an untouched default. An
  acceptance against a picked zone refuses typed, and the picked zone
  stands.
- **The dismissal is skippable and final for the value.** The offer's
  dismiss clears the question without answering it; the latches stand,
  so the dismissed value never offers again this process and the
  first-run offer never returns.
- **The host resolves once, at boot.** The OS exposes the zone as a
  readable value but no change notification a windowless host piece
  can ride, so no while-running watch ships; the core's latch accepts
  any later report, so a future watch drops in without shape changes.

## The message timestamps

The message timestamps' display zone stays UTC: the wire instant is
UTC and the list derives every label from its UTC parts, so a stamp is
stable wherever the app runs. Time-zone-aware display for the message
list is the named open half of the time-zone row, the wait this
document records.

## Known gaps

- The message timestamps' zone display stays UTC (the open half
  above).
- One catalogue ships (en-GB); the full app copy outside the settings
  and About surfaces is the localisation row's named remainder.
- The first localised set is the Diagnostics, Privacy, and About copy;
  the settings screen's own chrome stays English until its catalogue
  lands.
