# Showcase mode: the Astra Mail contract

**Scope.** The debug flag that boots the fixture-backed shell for store
screenshots: one environment variable chooses a recorded screen, the
desktop host answers the recorded snapshots instead of dialling anything,
and the production webview bundle runs unmodified. Debug builds only; the
gaps are recorded at the end, never left silent.

**Principle.** *The shipped binary cannot have its surfaces swapped.* The
whole showcase path compiles only in debug builds, so no environment
variable of a release build can put recorded answers in place of a live
account.

## The flag

`ASTRA_SHOWCASE` names the one screen the run photographs: `list`,
`reading`, `settings`, or `people`, matched literally (case folded,
surrounding space trimmed). Unset, or one of the off words (`0`, `false`,
`no`, `off`, empty), means the production boot. An unknown name is
refused with the name in the error rather than silently photographed as
the list: a well-lit capture of the wrong screen is the failure nothing
downstream can detect.

## The fixture-backed host

When the flag stands, the boot assembles the fixture app instead of the
production one: no account dial, no engine store, no LLM client. The
command layer answers the recorded snapshots the web fixture harness
carries, compiled in from the fixtures the web tests already own: the
mailbox list for every screen, the reading snapshot standing open for the
reading screen, the settings snapshot for the settings screen, and the
people snapshot for the people screen. The production webview bundle runs
unmodified against those answers; the alias rule that keeps the test
harness out of production stays intact. The showcase's own corner under
the data directory holds the account index, and it stays empty.

The reference client's in-memory provider boot is recorded as not taken:
it needs bindings code the vendored tree does not carry, against a
dial-first boot, and a screenshot run does not need real-engine fidelity.

## The frozen dates

The snapshots' dates stand as recorded. No clock is pinned, so a run
photographs whatever the fixtures carry: a seeded today approximated by
freezing the recorded days, not a live now.

## Known gaps

- **The fixtures are static records with no pinned wall clock.** The dates
  are frozen as recorded, so a screenshot run cannot stand on a chosen
  day.
- **Four screens stand.** List, reading, settings, and people; no composer
  or calendar screen is recorded yet.
