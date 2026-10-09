# SP2a Account Discovery Design

Date: 2026-10-09
Status: proposed
Parent spec: `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (sections 5, 15)
Discharges: the detect_account_settings deferral recorded in the SP1 spec section 6 amendment

## 1. Purpose

Complete the account setup story for the manual-plus-discovered path: port
mail-calendar's autodetection (JMAP probe, autoconfig, ISPDB, SRV, MX
fallback), expose it through the host as `detect_account_settings`, and let
the wizard offer the recommended setup with manual override. OAuth providers
(Graph, Google, JMAP password flows beyond detection) and account removal
stay with later slices.

## 2. Decisions

- **D1**: port `mailcal-autodetect` as `crates/astra-autodetect` (strategies:
  JMAP well-known probe with `_jmap._tcp` SRV, Thunderbird autoconfig,
  ISPDB, IMAP/SMTP SRV, host-DNS MX fallback; the untrusted non-HTTPS
  approval gate per `docs/account-autodetect.md`; email never appears in
  URLs).
- **D2**: port the recommendation layer from `mailcal-account`
  (`recommend(email, detected, routes) -> SetupRecommendation`,
  `SetupChoices`) so the wizard renders the same choice set mail-calendar
  ships (recommended provider plus toggles for manual entry).
- **D3**: the host gains `detect_account_settings(email)` returning the
  recommendation as an IPC type; the wizard calls it first and falls back to
  manual IMAP on any error, timeout five seconds.
- **D4**: no new Intents (detection is setup-path, like
  `imap_test_connection`); the sanctioned setup-path exceptions list in the
  SP1 spec section 9 amendment grows by exactly this command.
- **D5**: live coverage: the Stalwart harness exposes JMAP on its standard
  port; extend `harness.sh env` with the JMAP URL and add a gated test
  asserting the JMAP strategy detects a JMAP account for a harness user; the
  existing slice check stays green.

## 3. Testing

- Unit: strategy parsers ported with their MC test cases (autoconfig XML,
  ISPDB entries, SRV records, MX fallback ranking).
- Live (gated, harness): JMAP detection for `astra1@example.test`.
- Frontend: wizard test covering detect -> recommendation rendered -> manual
  toggle; detect failure falls back to manual.

## 4. Acceptance criteria

1. Gate, workspace tests, desktop build, web build and vitest all green.
2. `detect_account_settings` returns a recommendation whose choices match
   the manual-IMAP defaults for an unknown domain (unit-pinned) and a JMAP
   recommendation for a harness user (live, gated).
3. The wizard uses detection first with manual fallback; no regression in
   the existing manual flow tests.
4. `docs/capabilities.md` rows move accordingly (auto-detect row to
   partial; wizard row notes discovery) with the zh mirror and a changelog
   fragment in the same change.
5. The SP1 spec's known deferral is discharged: this spec's merge closes it.
