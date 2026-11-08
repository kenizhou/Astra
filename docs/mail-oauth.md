# IMAP/SMTP OAuth sign-in: the Astra Mail contract

**Scope.** How an IMAP/SMTP account whose provider offers a browser
sign-in is connected: what the setup screen asks the server before it
asks the person, where the authorization server is learned from, how the
client registers itself, and how the account behaves once its token
starts expiring. Binding on every surface that offers the mail sign-in
and on every account that consumes its grant. Ported in substance from
mail-calendar's `mail-oauth.md` (the eleven rules), scoped to what Astra
Mail ships today; the divergences are recorded as they are met and the
gaps sit at the end, never left silent.

[`provider-oauth.md`](provider-oauth.md) is the contract for Microsoft
and Google, whose endpoints and client registration a build holds at
compile time. This one is for every other provider: a server met cold,
discovered entirely from the standards, named by no vendor.

**Principle.** *The server is asked first, and every miss is the password
field.* A domain may publish an authorization server for its web sessions
and take only a password on IMAP, and a sign-in offered on that evidence
mints a token the mailbox refuses. So the mailbox's own capability line
decides, nothing is presented to it, and a question that goes unanswered
resolves to the one answer that works everywhere.

## The rules

1. **The server is asked first.** Before any OAuth discovery, the setup
   path opens one TLS connection to the mail server the person named and
   reads its pre-authentication `CAPABILITY`
   (`astra_account::imap_auth_offer` over the engine's probe verb).
   Nothing is presented and no failed sign-in is recorded: the probe
   stops at the capability line, closes with `LOGOUT`, and never sends a
   credential. The dial runs under the trust the wizard built, so a
   server whose certificate was accepted is asked exactly as the connect
   that follows is dialled; one accepted certificate covers the IMAP leg
   and the SMTP submission leg alike, because both name the one server.

2. **Three answers, because they are three screens.**

   | The server says | Setup offers |
   |---|---|
   | An OAuth mechanism, and an authorization server was found this install can register with | **Sign in**, primary. The password field, if a password also works, behind a secondary control. |
   | An OAuth mechanism, but no usable authorization server | A line saying the provider admits only applications it registered in advance, plus the password field. |
   | No OAuth mechanism, or nothing (unreachable, timed out, the certificate not yet accepted) | The password field, exactly as before. |

   The middle row is the reason this is an enum and not a flag. A
   provider whose sign-in exists but is closed to this application is
   not the same as one that offers none, and one bare password form for
   both leaves a person wondering why the button their colleague has is
   missing. **Behind means not drawn until asked for**: the sign-in
   button stands in the content with "Use a password instead" beneath,
   and choosing that draws the field; the field is never removed, and is
   hidden entirely only where the server itself admits no password.

3. **A password is offered only where one works.** `AUTH=PLAIN` or
   `AUTH=LOGIN`, or the absence of `LOGINDISABLED` on the IMAP leg; on
   the submission leg the `AUTH` line alone, because submission
   authenticates over SASL or not at all. A server that has switched
   password authentication off gets no password field: it would be a
   dead end nobody finds until they have typed one.

4. **An issuer comes from the provider describing itself, never from a
   third party.** Two channels, in order: the `<oAuth2><issuer>` of the
   provider's own autoconfig, parsed and trust-filtered upstream (the
   ISPDB's block and any untrusted hop dropped before the field
   crosses); failing that, the RFC 8414 well-known of the **email
   domain** and then of its registrable domain. Only the issuer is ever
   taken from a document; the endpoints beside it are ignored, and the
   real ones come from that issuer's own metadata. A JMAP server is
   asked differently and only there: it is an HTTP resource, so its own
   unauthenticated `401` names its authorization server (RFC 9728), a
   surface IMAP has no analogue of, which is why the probe above exists.

   **Recorded choice: the registrable candidate is the email domain's.**
   The reference words the second well-known candidate as the mail
   server's registrable domain; the spec's wording, which this build
   implements, is the email domain's. The two coincide whenever the mail
   server lives under the address's own registrable domain, which is
   every provider a person meets by typing their address. They diverge
   on a custom-hosted domain (an address at `mydomain.example` whose
   mail is hosted under an operator's domain): there the reference would
   find the operator's issuer and this build answers
   RegistrationNeeded with the password standing, the honest screen for
   a provider this build cannot name. The reference's shape is revisited
   if a hosted domain is ever met in the wild.

5. **A client id is registered, never invented.** Where the metadata
   advertises a `registration_endpoint`, this install registers itself
   (RFC 7591) and the issued id persists with the account, so a launch
   never re-registers and a server that later withdraws open
   registration cannot break an account that already has one. Repeated
   attempts inside one session reuse the id already minted: without
   that, three taps of a flaky sign-in leave three orphaned client
   registrations on the person's account.

6. **A provider that will not do open registration needs an entry this
   build carries, and absent is a supported build.**
   `astra_oauth::static_mail_providers` holds the endpoints and host
   patterns, matched on the server host at a label boundary; the client
   id is injected at build time, and a route whose id is absent is not
   offered. A provider that issues only a confidential client is one to
   raise with them, not to work around: an installed binary cannot keep
   a secret, and embedding one to look compliant is worse than the
   honest message rule 2 already shows.

   **Recorded choice: the table ships empty and its precedence is
   static-first.** An entry this build holds beats discovery, because a
   pre-registered client carries the provider-reviewed scopes a
   dynamically minted one lacks. An entry whose client id is blank (the
   injection absent) is skipped rather than treated as a route, leaving
   discovery to answer, so an entry can ship its endpoints before its
   registration exists without closing the server's discoverable route.
   Until an entry exists, rule 2's middle row is what every
   closed-registration provider gets, and it says something true rather
   than nothing.

7. **Every failure is the password field.** A refused dial, a metadata
   document that will not parse, a registration refused, a server that
   never answers, a certificate not yet accepted: all of them mean the
   question went unanswered, and the answer that works everywhere is a
   password. A setup screen never shows an error for any of this; which
   step gave up is in the diagnostic log, where it is useful.

8. **The card shows nothing to act on until the server answers, and a
   deadline races the ask** (10 s, the host command's own bound, with
   the card's belt-and-braces race beside it). The ask waits for the
   fields to settle (400 ms; a probe per keystroke is a question the
   server never needs asked), the decision bounds its own work inside
   the deadline (3 s for the capability probe, 6 s for the issuer
   search, where running out is a server that did not answer and an
   issuer not found), and only the last answer that landed draws the
   screen: a re-ask over an edited field holds the standing screen, so
   a credential field is never drawn and then taken away. The ask never
   needs the password: the host builds the question over a grant-shaped
   credential, so the server can be asked before anything is typed.

9. **The credential is resolved per dial, and one authentication failure
   is worth one re-dial.** An access token expires within the hour while
   an IMAP session does not, so a config built once would authenticate
   for exactly as long as its first token. The engine deliberately
   refreshes nothing itself: every dial of the account asks the shared
   token source (single-flight, replay cool-downs, rotation through the
   host's sink) for a token valid at that moment, and a refused token is
   renewed exactly once through the same source before the dial gives
   up, so a burst of refusals spends one refresh, not one per leg. The
   SMTP submission leg presents the same token over its own mechanism
   selection. On a password account none of this runs: the same secret
   would go back to the same server, at a provider that may be counting
   attempts toward a lockout.

10. **An OAuth account stores no password anywhere.** The grant is
    stored once, as the `[imap.oauth]` block of the account's document
    (the client id, the refresh token, the discovered endpoints, the
    redirect, the scopes, the issuer where RFC 9207 applies), and the
    SMTP leg presents the same source rather than a reused password.
    "Repair this account" is a re-authorisation, never a typed secret,
    and the family never shares a grant with JMAP or the integrated
    families: one account, one credential, one refusal, one remedy.

    **Recorded choice: no mail-access scope gate on this chain.** The
    JMAP chain refuses a metadata document whose advertised scopes grant
    no mail access, because a JMAP token good for nothing would surface
    much later as an empty mailbox. This chain carries no such gate: the
    reference does not require one for SASL, where a server may accept
    the token and apply its own default grant, and the SASL legs answer
    a useless scope as a refused authentication, which the expired row
    already names. The asymmetry with JMAP's chain is deliberate and
    recorded here rather than smoothed over.

11. **`iss` is checked where a server says it sends one** (RFC 9207).
    That is the only thing that catches a mix-up relay once `state` has
    matched, and it is checked strictly: a response missing the
    parameter from a server that advertised it is refused, because
    deleting a query parameter must not be all it takes to switch the
    check off. A server that advertised nothing has nothing to compare,
    which is the pre-RFC-9207 status quo, not a fault.

## The grant, the dial, and the surfaces

The completion follows the JMAP model exactly: the token source
registers with the dial table's entry **before** the dial, so a rotation
that lands mid-dial has somewhere to go and the final persist reads the
entry, never the copy parsed a moment before the dial. A first add
persists through the transaction's `add` and rolls the whole
registration back on a refused dial; a re-consent for an account that
stands goes through `rotate`, keeping the stored shape beside the grant.
A grant whose token set carries no refresh token is refused at the first
door, `offline_access` having been requested, and the refusal surfaces
with its classification word, never as silence.

At launch the boot dial classifies a dead grant into the expired-sign-in
state for the account it then skips: the accounts page renders the
imap_smtp family's row naming the refused credential (never an outage
badge), and its remedy is reconnecting the account: the mail grant does not
share the JMAP family's re-authorisation arms (those are typed over
the JMAP section and its grant shape), so the mail family's own
re-grant arm is a deliberate later slice, not an expectation.

## What is deliberately not done

- **No RFC 7628 challenge probe.** The challenge a server returns when
  it rejects a token may describe its OAuth configuration, so a client
  could discover the authorization server by presenting a deliberately
  invalid one. It would leave a failed authentication on the person's
  account before they had signed in once, on the screen they are most
  likely to abandon, and the field is optional: Stalwart answers a bad
  token with a bare `NO [AUTHENTICATIONFAILED]` and no challenge at all.
- **No RFC 8707 `resource`.** The profile defines no URI form for an
  IMAP endpoint; a server that scopes tokens by resource applies its
  default without one, and inventing `imap://` would risk
  `invalid_target` on the exchange and on every refresh after it. A JMAP
  account still sends the one its RFC 9728 metadata published.
- **No mechanism knob.** Which SASL mechanism carries the token is
  negotiated by the engine from what the server advertises, and a caller
  able to pin one would be encoding which provider it is talking to.

## The proof

The offline suites carry the whole chain over scripted servers: the
three answers over observed capability lines, the issuer candidates in
order, the registration cache, the grant's round trip, the per-dial
credential and its one renewal, the completion's ordering, the dead
grant's row, and the screens' rendering. The gated live suite pins what
only a real server can give, against the local Stalwart harness: the
capability line on both legs through the engine's own probe verbs (the
IMAP pre-auth line and the submission leg's EHLO AUTH line, read under
the product's own TLS acceptance walk), the decision composed over the
real dialled server into the registration-needed shape, and the
discovery's fail-soft over the harness's resolving-nowhere domain, the
issuer chain answering its miss inside its budget and the un-accepted
certificate's refused dial answering the password screen.

A full sign-in round trip is not provable there: the harness serves no
OAuth issuer, the JMAP slice's finding. That proof waits on a real
identity provider, exactly as JMAP's does, and nothing fakes it.

## Known gaps

- **The static provider table ships empty**: no registered provider id,
  no offered route, the honest middle row for every closed-registration
  provider until an entry exists.
- **The full chain's live proof waits on a real identity provider** (the
  harness serves no issuer); the probe and the discovery's fail-soft are
  the live pins, and the complete chain runs over scripted servers
  offline.
- **The password family's expired row is the password family's own** (a
  refused password), unchanged: the account rows already report it, and
  the imap_smtp row this slice adds names the refused OAuth credential.
- **The RFC 7628 challenge probe and the RFC 8707 resource stay out, and
  no mechanism knob exists**, exactly as the deliberate cuts above
  record.
