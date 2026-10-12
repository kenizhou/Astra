# SP2j Settings Design

Date: 2026-10-12
Status: proposed
Parent spec: `docs/superpowers/specs/2026-08-...-workflow-design` (sections 5, 15)
Port contracts: mail-calendar `docs/settings.md` in substance (the taxonomy's
shape and its per-category rules), scoped to what Astra ships today plus the
recorded gaps that name this slice (the wizard's dav field; the calendar's
persisted shape and zone; the appearance persistence).

## 1. Purpose

Settings as one surface: the unified screen with the categories Astra has
settings for, persisted in the core's preferences document and consumed by the
core where the core needs them (the calendar's display zone, the first day of
the week, the horizon), the Accounts page with the dav-section editor that
closes contacts.md's wizard gap, and About.

## 2. Decisions

- **D1 (the surface)**: a `Surface::Settings` carrying the `SettingsSnapshot`
  (the persisted settings block, the account rows, the about block). The
  shell's TitleBar gains the entry (a gear beside the panels). The screen
  presents as the desktop pattern: a category source list beside a detail
  panel, modal over the shell.
- **D2 (the family)**: `Settings(SettingsIntent)` with `Refresh`, `SetTheme
  { mode }`, `SetClock { twenty_four: bool }`, `SetZone { zone }`,
  `SetFirstDayOfWeek { weekday }`, `SetHorizon { days }`, `SetQuoteStyle
  { below: bool }`, `SetViewMode { mode }` (the grouping moves to settings;
  the existing list arm delegates), `SetDefaultCalendar { calendar }`,
  `SetDav { account, section }` (the editor; `None` clears), and
  `RemoveAccount { account }`. Every intent persists into the preferences
  document before republishing.
- **D3 (persistence and consumption)**: the settings live in the preferences
  document (TOML, additive fields with defaults; an older file loads clean).
  The core consumes what is its: the calendar's display zone (replacing the
  hardcoded `Etc/UTC` pending note), the first day of the week (the grid
  layout and the create path's WKST), the horizon default (the cold window),
  the default calendar (the create dialog's preselected writable target).
  The host chrome consumes the rest (theme, clock, quote style, grouping).
- **D4 (the Accounts page)**: one row per account (address, kind, what it is
  used for incl. the dialled legs), each opening its own page: the dav
  section editor (origins + username + password; CardDAV falls back to the
  CalDAV origin as the dial does; saving re-dials and installs the legs
  without a restart; a failed re-dial keeps the old legs standing and says
  so), and Remove account (destructive, confirmed by naming the address,
  removing the stored config and tearing down the account's state; the mail
  half and the legs go together).
- **D5 (About)**: the version (the `VERSION` file and the workspace version,
  one source) and the attributions (the engine, the sidecar's pi SDK, the
  web stack), the core's answer not the web's guess.
- **D6 (appearance)**: theme follows the host by default with Light and Dark
  as an explicit override, persisted; the clock is 12 or 24 hour, persisted,
  consumed by the grids' labels and the reading timestamps.

## 3. Testing

- Unit (astra-app over fakes): the settings intents persist and republish;
  the preferences round trip with defaults for absent fields; the calendar's
  consumption (a set zone flows into the snapshot's `now_data.zone` and the
  expansion; a set first day flows into the grid window and the create
  WKST); the dav editor (a good section re-dials and installs; a failed
  re-dial keeps the old legs and answers the failure; clearing removes);
  remove account (the stored config goes, the account's rows and legs go).
- Wire: the SettingsIntent family + SettingsSnapshot literals pinned, both
  languages.
- Frontend (vitest): the source list and detail; the pickers (theme, clock,
  zone with the browser's IANA list, first day, horizon); the accounts page
  (the rows, the dav editor's confirm-and-failure states, the remove
  confirmation naming the address); About.
- Live (gated, harness): the dav editor against the running harness (edit
  the section, the re-dial installs both legs, the calendar and contacts
  fill); no other live need.

## 4. Known gaps (pre-recorded)

- The categories Astra has no feature for stay absent, never stubbed:
  Signatures, Notifications, Privacy/analytics, Diagnostics, swipe actions,
  default send account, per-account fetch depth and message size, the sender
  name field, localisation, the Allodia account, default mail app.
- Advanced (the reset and the MCP panel) waits for the MCP slice.
- The zone list is the browser's own (`Intl.supportedValuesOf`); a zone the
  host cannot resolve is refused typed.
- The settings screen is desktop-shaped (source list beside detail); no
  hub-and-spoke variant is built.
