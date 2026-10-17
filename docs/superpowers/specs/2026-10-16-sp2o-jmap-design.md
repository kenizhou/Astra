# SP2o JMAP Accounts Design

Date: 2026-10-16
Status: proposed
Parent spec: `docs/superpowers/specs/2026-08-...-workflow-design` (sections 5, 15)
Port contracts: mail-calendar `docs/jmap.md` (the rules section carried
whole) and `crates/mailcal-account/src/jmap/` as the code reference; the
OAuth half rides the SP2l floor; `mail-oauth.md`'s XOAUTH2 half stays
SP2v's.

## 1. Purpose

The third provider over the OAuth floor and the first whose live proof
needs no owner registration for the password leg: a JMAP account is a
base URL and ONE secret (a password or an API token, stored as
`password` because that form is strictly more capable), connected either
by the pasted secret or, where the server advertises it, by the
standards-discovered OAuth sign-in. The harness speaks JMAP natively, so
the password leg is live-provable the day it lands.

## 2. Decisions

- **D1 (one secret, one form)**: the stored `[jmap]` section carries the
  email, the base URL, and one `password` (Secret). The setup's JMAP
  arm asks for one field labelled "Password or API token", never two; a
  legacy `token` key still reads. The engine's transport negotiates the
  scheme from the server's own 401 challenge.
- **D2 (the URL derivation)**: a blank server derives
  `https://<email-domain>` over the well-known; a bare host gains
  `https://`; an explicit `http://` survives only for the harness
  fixture (the trust rule the detection already carries).
- **D3 (the account identity and dial)**: the id keys
  `address@jmap:<host>` (distinct from the IMAP and the OAuth
  families); ONE provider covers the whole account (the mail scope is
  account-wide; folders sync through it; the folder binder reconnects
  the same account-wide provider). The dial assembles the engine's
  `JmapProvider` behind the shared TLS and retry policies.
- **D4 (the OAuth leg, where advertised)**: the SP2l floor's discovery
  (RFC 9728 -> 8414 -> 7591) offered when and only when the server's
  own 401 names it; every hop HTTPS (loopback excepted), the issuer
  matching, S256 required; the scopes limited to what the app uses; the
  discovery persisted once (the endpoints, the client id, the refresh
  token in `[jmap.oauth]`); a `RefreshingJmapProvider` wrapper for the
  OAuth account (re-mint ~hourly; the rotation write through the SP2l
  transaction, which is NOT optional on a ratcheting JMAP server). A
  re-auth (the expired grant's own button) re-runs from the persisted
  grant with no second registration.
- **D5 (the wizard arm)**: the detection's existing JMAP recommendation
  routes to a JMAP form (the one field, the derived URL editable); the
  OAuth offer renders only where the server advertises it (the
  fail-soft rule: any discovery miss leaves the field there); the
  OAuth's four binding rules port as rules.
- **D6 (the surfaces)**: the account's rows (the kind, the legs), the
  expired-sign-in row (the OAuth account's own re-grant; the password
  account's row points at Settings), the calendar read riding the
  provider's own advertised capability.

## 3. Testing

- Unit (astra-account): the config section (one secret; the legacy
  token read; the URL derivation matrix); the dial over the scripted
  session; the refreshing wrapper's mint/rebuild discipline (the
  Graph/Google shapes); the OAuth discovery's four binding rules
  (already partly proven in astra-oauth's battery).
- The wizard (vitest): the JMAP form, the one field, the OAuth offer's
  advertise-gated render, the fail-soft.
- Live (gated, the harness's JMAP on 2080): the password leg END TO
  END (connect, the folder list, the message list, a body read, a
  send); the calendar read where the harness advertises it; the OAuth
  leg's discovery against the harness's 401 (the RFC 9728 document
  exists; whether Stalwart advertises the full chain decides the
  honest pin or gap).

## 4. Known gaps (pre-recorded)

- Contacts over JMAP ride the contacts slice (the engine's adapter
  exists; MC's refreshing_contacts.rs defers with it).
- The sender-name write and the per-use capability choices ride their
  own slices.
- The OAuth leg's live proof depends on what Stalwart advertises; the
  password leg is fully live-provable regardless.
- IMAP/SMTP XOAUTH2 stays SP2v's.
