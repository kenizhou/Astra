# SP2t design: the platform batch

**Status.** Decided 2026-10-24. Slices branch from main at this commit.

**Scope.** The platform batch the backlog names: diagnostics and the log
viewer, consented analytics, the privacy surface, OS integration, the
localisation beginning, and the device-zone prompt. Today the windowed
build logs into the void (no logger at all), no diagnostics surface
exists, no analytics, no privacy statement, no autostart/tray/window
memory/default-mail offer, all copy is hardcoded English with dates
pinned to en-GB, and first run lays the calendar out in UTC because
nothing reads the device zone.

## D1: diagnostics

The host installs a production logger at boot: a hand-rolled rotating
file log (the headless StderrLogger's sibling, writing the app data
dir), capped in files and bytes, honouring the log contract the codebase
already holds (no secrets, no mail content, no repository paths). The
debug level is a persisted setting (the default info; debug raises it
after the next launch, the honest wording). The settings screen gains a
Diagnostics category: the log tail rendered in-app (a host command
reading the last N lines), Copy for support (the tail plus the version,
with the privacy note naming what logs never carry), Export (the save
dialog over the whole current file), and the debug toggle. logging.md is
created with the zh copy.

## D2: consented analytics

Opt-in, default off, one-click withdrawal, and no mail content in the
payload, exactly as the row demands; and the sink ships local: the
consented events land in the app's own AI ledger (the existing sqlite,
counts and shapes only) and NOTHING leaves the device, because no
endpoint exists and inventing one is the owner's call, not a port. The
row ships partial with that truth named: the consent machinery, the
no-content payload, and the withdrawal are the shipped half; the sink
and the reporting endpoint are the wait. analytics.md is created with
the zh copy.

## D3: the privacy surface

A Privacy settings category: the plain statement of what stays on the
device (the stores, the ledgers, the logs' contract), what the AI
features send (the relayed prompts, per ai-rules.md), what plugins may
ask for, and the two switches that live here beside the statement (the
analytics consent of D2 with its withdrawal, and a link to the plugin
registry's own policies). The category is the row's home; the statement
is written in the house voice, no legalese.

## D4: OS integration

Three legs: the default-mail-app offer (the row's ask-once, changeable
in settings: the host registers the mailto association on the offer's
acceptance through the deep-link plugin's runtime registration; the
live OS-handoff proof waits on an installed build exactly as the mailto
row already records), window state persistence (the main window
remembers size, position, and maximised across launches, hand-rolled
over the app data dir, no new plugin), and the share sheet (the OS
share-target entry handing shared files into a new prefilled message,
the mailto channel's sibling). os-integration.md is created with the zh
copy.

## D5: the localisation beginning

The language setting ships (English today, the picker honest about
being the mechanism, not a catalogue of translations), and the DATES
follow it at once: the hardcoded en-GB formatters read the setting, the
weekday labels and the sent-at stamps localise, the calendar's display
zone untouched (its own setting). The copy catalogue begins with the
settings and About surfaces as the first localised set; the full app
copy is the named remainder the row carries. timestamps.md is created
with the zh copy owning the dates contract.

## D6: the device-zone prompt

At first run, when no zone has been picked and the default UTC stands,
the diary offers the device's own zone (resolved web-side, validated
through the core's arm, accepted into the standing setting); the offer
is once (a persisted latch), skippable, and never overrides a picked
zone. On a device-zone change while the app runs, the same offer stands
once per change. The message timestamps' display zone is NOT this
slice's (row 120's display half rides timestamps.md with the
localisation dates).

## D7: showcase mode

A build-gated showcase mode that boots a seeded in-memory dataset for
store screenshots: the headless harness's fixture machinery driven from
a launch flag, the honest scope being the shell rendering a full diary
and list with no accounts and no stores written. If the leg proves
larger than the batch, it is recorded as the row's named shortfall
rather than silently dropped.

## Known gaps recorded, not hidden

- The analytics sink is local-only; no endpoint, no shipment.
- The default-mail registration's OS-level proof waits on an installed
  build (the mailto row's standing wait).
- The localisation ships the mechanism, the dates, and the first
  localised surfaces; the full copy catalogue is the named remainder.
- The tray and autostart stay absent (no row names them this slice).
- The message timestamps' zone display stays UTC (timestamps.md's row).

## Test-first

Every behaviour lands with its red: the logger's rotation and cap, the
tail/export commands' shapes and the privacy contract over their bytes,
the consent gate (no ledger row without consent; withdrawal stops the
flow), the language preference driving the date formatters, the
device-zone offer's latch and refusal to override, the window-state
round trip, the share-target prefill, and the showcase flag's gated
boot. The live proofs: the log file observed rotating under a write
burst over the headless harness, the consented events observed landing
in the ledger only while consent stands, and the export's bytes
matching the file.
