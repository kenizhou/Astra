# SP2v design: IMAP/SMTP OAuth sign-in

**Status.** Decided 2026-10-25. Slices branch from main at this commit.

**Scope.** The mail-oauth row: IMAP/SMTP sign-in with the provider where
the server offers it, discovered from the standards. The engine ships
the whole bearer half (the pre-auth probe, SASL OAUTHBEARER and XOAUTH2
on both legs, the per-dial credential source with one renewal); the
reference contract (mail-calendar's mail-oauth.md, eleven rules) is the
port. Astra adds the grant, the discovery chain, the dial arm, and the
screens; the empty static-provider table ships empty, exactly as the
reference rules.

## D1: the ask and the three screens

The server is asked first: the wizard's IMAP step probes the dialled
server's pre-auth CAPABILITY and EHLO (the engine's verb, nothing sent,
the connection closed). Three answers make three screens: OAuth
offered stands the sign-in button PRIMARY with the password field
demoted behind a secondary control (never removed); OAuth offered but
no route registered shows the "only pre-registered apps" sentence with
the password standing; no OAuth shows the password form as today. The
password is offered only where the server admits it (AUTH=PLAIN/LOGIN
or no LOGINDISABLED); a server that admits neither is the existing
unusable-server error.

## D2: the discovery chain

The issuer comes from the provider's own autoconfig `<oAuth2><issuer>`
(the autodetect crate already parses and trust-filters it: ISPDB and
untrusted hops dropped), else the RFC 8414 well-known of the email
domain and then its registrable domain. The RFC 9728 challenge route
is JMAP's leg and stays there. Dynamic registration (RFC 7591) mints
the client when the server offers it, session-cached like JMAP's; the
static mail-providers table ships EMPTY, and an absent client id means
the route is not offered, never invented. Every failure is the
password field.

## D3: the grant and the dial

A signed-in IMAP account stores an OAuth grant beside its section (the
JmapOAuthGrant shape model: client id, refresh token, endpoints,
redirect, scopes, issuer), no password. The dial builds the engine's
OAuth2 credential over the shared GraphTokenSource (single-flight,
cool-downs, rotation through the sink), asked once per connection with
the engine's own one-renewal retry; the re-dial on a refused token is
the engine's, not the reconnect wrapper's. The SMTP submission leg
presents the same token over its own mechanism selection. A dead grant
at boot records the expired-sign-in state (the imap_smtp kind's row
naming the refused credential; the family never shares a grant with
JMAP or the integrated families).

## D4: the surfaces

The wizard's three screens (D1); the completion follows the JMAP
model (the dial-table entry registered before the dial so a mid-dial
rotation lands; the add-versus-rotate transaction); the accounts page's
expired row gains the imap_smtp family's honest shape (the refused
credential named; the remedy reconnect-the-account until an in-place
grant ships, matching the JMAP row's shape). `iss` is checked strictly
where advertised; `resource` is NOT sent (the reference's deliberate
no), and no mechanism knob exists (the engine's preference order
stands).

## Known gaps recorded, not hidden

- The static mail-providers table ships empty: no registered provider
  id, no offered route, the honest row state.
- The harness serves no OAuth issuer (the JMAP slice's finding): the
  live proof pins the probe's three answers over the real server and
  the discovery's fail-soft; the full chain's live proof waits on a
  real IdP, as JMAP's already does.
- The password family's expired row (a refused password) stays
  unchanged (the account rows already report it).
- The RFC 7628 challenge probe and the RFC 8707 resource stay out,
  exactly as the reference rules.

## Test-first

Every behaviour lands with its red: the probe's three screens over
scripted offers; the issuer chain (the autoconfig route, the two
well-known candidates, the refusal shape); the registration cache; the
grant's round trip and the no-password rule; the OAuth2 credential's
per-dial ask and the one-renewal retry; the dead grant's expired row;
the screens' rendering (the password demoted, never removed); and the
live proofs: the harness's real capability line pinned (both legs'
mechanisms), and the discovery's fail-soft over the harness (no issuer
resolves).
