# Provider OAuth: the Astra Mail contract

**Scope.** How a browser sign-in is run at a mail provider, how the tokens it
mints are held, and how they are refreshed: for providers Astra Mail holds a
registration for (Microsoft 365, Google) and for servers met cold, discovered
entirely from the standards (RFC 9728, RFC 8414, RFC 7591). Binding on every
surface that offers an OAuth route and on every account type that consumes
one. Ported in substance from mail-calendar's `provider-oauth.md`, scoped to
the foundation Astra Mail ships today; the gaps are recorded at the end,
never left silent.

**Principle.** *The app is a public client: it has no secret worth stealing,
keeps the one secret it does hold in the OS keystore, and runs its sign-in in
the person's own browser session.* Every rule below is that sentence made
checkable.

## The two halves

The OAuth state machine is split by what can be tested purely and what is
inherently platform-specific:

- `crates/astra-oauth`, the pure half. PKCE and CSRF state minting, the token
  exchanges (the exact form bodies, the clock skew, the granted-set write
  rules), the Microsoft and Google endpoint and scope tables, the
  grant-refusal vocabulary, the transport reach split, and the discovery and
  registration chain. No engine dependency; every network call rides an
  injected HTTP client built on the shared TLS policy (`engine_tls`), so
  OAuth trusts exactly what sync trusts.
- `client/desktop/src/oauth`, the host half. The sign-in session (open the
  system browser through an injected seam, capture the deep-link redirect,
  hand the raw URL back), the credential transaction over the host's
  `AccountCredentialStore` port, and the wizard's sign-in commands with
  their typed completion event.

The engine is OAuth-agnostic: it takes a bearer access token and nothing
more, so token acquisition and refresh never cross `engine-api`.

## The rules

1. **Authorization Code with PKCE, public client.** The app is a public
   client; the PKCE `code_verifier` (S256) is what protects the token
   exchange, minted by the system CSPRNG. The client id is not a secret and
   may be embedded. No confidential secret is ever embedded, with one
   documented exception: a Google Desktop client (the loopback flow) carries
   a non-confidential `client_secret` that Google's token endpoint requires
   anyway, on both the code exchange and the refresh; Google's own guidance
   says it "is obviously not treated as a secret". See rule 13.

2. **The system browser, never an in-app WebView.** Sign-in runs in the
   platform's system browser session, not an embedded WebView. This reuses
   the person's existing browser login (often skipping a password prompt)
   and keeps the app out of the credential path. The session is
   non-ephemeral on purpose: it wants the shared browser session. On the
   desktop host the browser is the OS default, opened through the
   opener-backed seam.

3. **A custom-scheme redirect, validated by `state`.** The redirect returns
   to the app's own scheme (`com.kenizhou.astra`, the identifier
   `tauri.conf.json` already carries), registered beside `mailto` in the
   deep-link block; the redirect URI the provider configs use is
   `com.kenizhou.astra://oauth`. The capture filters by the same scheme and
   hands the raw callback URL to the pure half whole; the host never parses
   tokens itself. The pure half rejects a callback whose `state` does not
   match the one it minted (a possible CSRF), a callback missing the code,
   and, for a server that advertises it sends one (RFC 9207), a callback
   whose `iss` names any other authorization server: that is the mix-up
   defence, and an absent `iss` from such a server is a failure, not a pass,
   because a stripped `iss` is precisely what the attack produces.

4. **Tokens live only in the OS keystore.** The stored account credential
   carries only the refresh token, the single at-rest secret; access tokens
   are minted on demand and never persisted. The secret type redacts itself
   in logs and debug output. Storage is the host's OS secure store (Windows
   Credential Manager today), the same store password accounts use.

5. **The credential's lifecycle is a transaction, and it answers.** A stored
   credential is created, replaced and erased through one transaction over
   the host's `AccountCredentialStore` port, with `persist` and `delete`.
   The host owns what only it can (the encryption, the access control, the
   store item); the rules below own when.

   **Where the transaction lives diverges from the reference, deliberately.**
   mail-calendar put the transaction in its core, under "the core owns
   when, the host owns where". Astra Mail's transaction is host-side,
   wrapping the port, for three reasons recorded with the module: the port
   itself is host-side and the app core sees it only through the
   account-operations seam, which exposes no generic persist; the app core
   carries no OAuth type at all, so a core-side transaction would widen the
   core's surface for a consumer that does not exist until the provider
   slices land; and the account-add flow already keeps its persist
   discipline host-side (`add_account` persists after the dial and rolls
   the whole add back on a refusal). The rules bind wherever the transaction
   lives. The refresh anchor: the Microsoft account's refresh path (SP2m)
   holds this transaction and answers its rotations; every rotation the
   token source hears lands in the dial table's entry and the store, and
   the completion's final persist reads the entry, never the copy parsed a
   moment before the dial, so a rotation during the dial is never undone.

   **A refresh names no scope, and that rule is not negotiable.** RFC 6749
   section 6 requires the requested scope on a refresh to be a subset of
   what was granted, and an omitted one to mean exactly the original grant.
   Sending the build's current list therefore breaks every grant issued
   before that list grew: the server answers `invalid_scope` and the whole
   account stops working, not merely the feature the new scope was for.
   Omitting it is the only value that cannot go stale. Scope is asked for
   on the authorisation request, which is where consent is given and what
   signing in again re-runs. The refresh body the pure half builds carries
   no scope parameter, byte for byte.

   **What a grant carries is the token response's word, and the stored copy
   follows it.** A grant's scopes are stored so that what the account may
   do can be answered at launch and offline, but the value is a cache:
   sign-in writes the scopes the code exchange named (the requested set
   when it named none, per RFC 6749 section 5.1), and every refresh that
   names a different set overwrites it, narrower included, since consent
   withdrawn at the provider reaches the app in no other way. A refresh
   that names none leaves it alone. An unchanged set is not written,
   because a secure-store write per refresh is a prompt on some hosts. The
   access token is never read for this: Google's is opaque, and
   Microsoft's belongs to the resource.

   **A refusal is classified, never matched on text.** A token endpoint
   refusal answers with a classification: `Dead` (`invalid_grant`: revoked
   or expired), `Underscoped` (`invalid_scope`: alive but narrower than
   this build needs), or `Indeterminate` (everything else, which says
   nothing about the grant at all). The first two both need consent and
   neither recovers by waiting, so a caller treats them alike and stops
   retrying; the third must never change what anyone believes about a
   sign-in, or a bad afternoon at a provider signs somebody out. The
   wizard's refusal sentences hang off the classification word, never off
   a sentence matched at the call site.

   **Replay safety: a server that treats a replayed refresh token as theft
   revokes the whole grant.** Modern authorization servers do not merely
   reject a superseded refresh token; some revoke the grant outright, so
   the account is dead and the person must sign in again. Microsoft and
   Google both forgive replays today, which is exactly why the rules exist:
   the first ratcheting server connected must not be the first to lose an
   account. Four halves hold this, split across the two slices:
   - **One refresh at a time per account**, and a refresh failure is shared
     with the waiting callers, not just a success. The token source that
     serializes refreshes and memoes its last failure lands with the
     provider slices (SP2m first); the seam it will consume, the transport
     reach split below, is already pinned.
   - **A failure is only retried with the same token when the request
     provably never left the device.** The transport split answers this:
     `NotSent` is claimed only where provable (DNS failure, a refused
     peer, a failed TLS handshake) and everything unclear is
     `MaybeProcessed`, deliberately asymmetrically, because being
     needlessly cautious costs a delay and being wrong the other way costs
     the account. Each carries its own cool-down, and both stay retryable,
     so a dropped packet can never put a re-authentication prompt in front
     of a user.
   - **A response that died after the status line is its own error**, never
     "transport": the request reached the server, its handler ran, and the
     refresh token presented is spent. Presenting the old token again is
     then a replay.
   - **A core that can refresh must be able to persist.** The token source
     takes the store as a required input rather than trusting a host to
     install one afterwards; a rotation no store receives is a stored token
     the server has already moved past. The transaction this rides is the
     one this rule 5 names.

6. **Discovery from the standards, fail closed.** A server met cold is
   asked, in order: its RFC 9728 protected-resource metadata (from the
   `WWW-Authenticate` challenge's pointer, else the well-known default),
   which names the authorization server; that server's RFC 8414 metadata;
   and its RFC 7591 registration endpoint when advertised, which mints a
   client id under a fresh `SOFTWARE_ID` so installs never misattribute.
   Discovery decides where a person's credentials are sent, so it is
   strict: every hop HTTPS (loopback the one exemption, because it never
   reaches a network), redirects not followed (a redirect moves the
   document to a URL the issuer did not name), the metadata's `issuer`
   must match the issuer asked about (RFC 8414 section 3.3, which is what
   stops a compromised resource pointing the app at an attacker's token
   endpoint), and S256 PKCE must be advertised. Any discovery failure
   means the same thing to a caller: this server does not offer
   discoverable OAuth, offer the manual secret instead.

7. **`offline_access` is mandatory.** Without it no refresh token is
   issued and the account breaks an hour after setup. The enforcement
   point, refusing a completed sign-in that yielded no refresh token, is
   the Microsoft completion's first door (SP2m); the rule is the contract
   the other providers inherit. Google's equivalent is not a scope:
   `access_type=offline` plus `prompt=consent` on every authorization
   request, so a refresh token comes back even on a repeat consent.

8. **Register the redirect URI in the provider portal,
   character-for-character.** It must equal what the client sends. The
   scheme is the app's own identifier; a dev-versus-shipped scheme split
   (the reference needed one where packaged and unpackaged builds claim
   the same protocol per user) has no analogue while one build identity
   exists, and is revisited when a second one does.

9. **Target the connected account with `login_hint`.** When the address is
   known, the authorization request carries it as `login_hint` and, for
   Microsoft, drops `prompt=select_account`, so the provider targets that
   account rather than a different one already signed in in the browser.
   With no hint, Microsoft gets `prompt=select_account` and shows the
   picker. This matters most when the browser holds a different account
   than the one being added.

10. **A failed or declined sign-in is surfaced, never swallowed.** The
   provider can return `access_denied` (consent declined, or a policy
   blocked the app); the sign-in surface shows the refusal with its
   classification word rather than doing nothing. A button that silently
   does nothing is a bug. The honest corollary for the providers without
   account wiring: Google's completion still answers the bare signed-in
   word and persists nothing, until its slice lands.

11. **A sign-in abandoned in the browser never wedges the surface.**
   Closing the browser tab is undetectable, so the way out is explicit:
   the session holds one in-flight slot, a fresh begin REFUSES while a
   flow stands (the surface always carries the cancel: the wizard's
   Cancel button and its close path both take the pair back, and the
   waiter a cancel displaces parks until a redirect or capture close,
   never blocking a later flow: the flow's id check hands a stranger's
   redirect back to the channel and the displaced waiter answers
   Cancelled without emitting anything). The cost of a cancel with no
   redirect: one parked waiter per abandoned flow, released at the next
   redirect or shutdown. A cap-and-supersede shape (mail-calendar's
   waiting-out) is a recorded gap, not shipped behaviour.

12. **The client registrations are injected at build time, and a build
   without one does not offer that route.** Google's and Microsoft's
   client ids enter the product in exactly one place,
   `astra_oauth::credentials`, read from the environment with
   `option_env!`:

   | Target | Variables |
   |---|---|
   | the desktop | `ASTRA_GOOGLE_DESKTOP_CLIENT_ID`, `ASTRA_GOOGLE_DESKTOP_CLIENT_SECRET` |
   | every target | `ASTRA_MS_CLIENT_ID` |

   These are Astra Mail's own registrations, not the reference's: none is
   shipped today, so the wizard's OAuth offer renders for no one and
   detection recommends no OAuth route; a from-source build is a working
   open-standards mail client, never a dead button. Absent is a supported
   build, not a broken one. A blank value is an absent value. The
   reference's refuse-a-shipped-build check is deliberately not ported
   yet: it turns an absent registration into a compile error for release
   builds, which is meaningless until a registration exists to be absent,
   and it is to be revisited the day one does.

13. **The scope sets name their calls.** A sign-in requests the scopes for
    what the account is used for, and every scope has a call site, which
    is what an administrator approving the app reads. The tables live in
    the pure half (`MICROSOFT_GRAPH_SCOPES`, `GOOGLE_SCOPES`) with the
    reasoning per scope; the shape of the reasoning:

    | Rule | Why |
    |---|---|
    | `offline_access` (Microsoft) or `access_type=offline` (Google) on every request | a refresh token at all (rule 7) |
    | `User.Read` / `userinfo.email` always | the account's own address and name; nothing reads an ID token, and no OpenID Connect scope is requested, because each would be a consent line with no feature behind it |
    | mail scopes (`Mail.ReadWrite`, `Mail.Send`; `mail.google.com`, `gmail.settings.basic`) | mail sync, every mail write, and send; send is never granted by the mail-read scope |
    | calendar, contacts read and write | the account's calendar and cards; contacts are requested read and write because the product adds and edits them and never deletes one |
    | directory scopes (`User.ReadBasic.All`, `directory.readonly`) | colleagues on a work or school account; never on a personal one, which has no directory |
    | a scope a user cannot consent to for themselves | never requested: an admin-gated scope does not cost one capability, it stops the account being added at all, because the `access_denied` lands during consent |

    A use the grant withholds is not opened; a stored granted set without
    the scope leaves that use closed. Google lets a person untick a scope
    on the consent screen, so the granted set is read back and compared
    as the provider spelled it. A widened set reaches an existing account
    only when it signs in again: a refresh re-uses the original grant.

## The Microsoft account

The first account type over this floor (SP2m): a Microsoft account signs
in through the browser flow, its mail and calendar dial through the
engine's Graph adapter over one token source, and the surfaces it owes
arrive with it. Everything here is scoped to what shipped; the gaps at the
end of the document carry the rest.

**The dial.** One free function in the account layer dials a Microsoft
account: mail providers, one per synced folder the account lists, and the
provider for the account's default calendar, appended after the mail the
way a dav account's calendar leg is, so the host's capability-scoped
selection picks it up. The boot, the sign-in's completion, and a reconnect
all call the same function, so no path can disagree about what a Microsoft
account opens. Both legs share the one token source, so a dial mints at
most one access token. The mail leg is the account's: its failure fails
the dial, and a dead grant arrives as a refused sign-in, never an outage.
The calendar leg is not: a calendar that fails to dial (a scope-denied
`403` on the probe, today matched on the server's `ErrorAccessDenied`
wording pending an upstream structural classification) leaves the account
standing on its mail with the failure carried for the caller to surface.
The refreshing wrapper caps what it advertises: a capability it does not
forward is a capability the account does not have, and server-side drafts
are deliberately not advertised (the recorded gap below).

**The address lookup.** The completion names the account from the
just-minted access token: `GET /me?$select=mail,userPrincipalName` with
the bearer token, the SMTP `mail` winning and the sign-in name falling
back where no SMTP address is set. A reply that names no address refuses
the sign-in: an account the provider cannot name does not become one, and
the person stays on the setup surface they are still looking at.

**The completion's ordering.** The account's id derives from the address
(`address@graph`), and everything keys on it. The token source registers
with the dial table's entry **before** the dial: the sink reads the entry,
so a rotation during the dial has somewhere to land, and the next launch
never presents a spent token. The final persist reads the dial table's
entry, not the copy parsed a moment before the dial, for exactly that
reason. A new id persists through the transaction's `add` (its own
rollback on a refused store); a re-consent goes through `rotate`, never
`add`, whose rollback would delete a standing account's credential.

**The re-consent.** Signing in again for an account that stands keeps the
stored shape beside the grant: the pinned id, the dav legs, the accepted
certificates. Only the grant changes. A failed dial on a re-consent
restores the standing entry as it was; a failed dial on a first add rolls
the whole registration back, leaving nothing registered and no orphaned
credential in the store.

**The door rule.** A grant the response names no scope for at all is
refused at the door, before anything registers: an account with no
permission cannot bear a single capability, and refusing early keeps the
person on the setup surface rather than walking them through an add that
ends in an unexplained folder-list refusal. The enforcement is the
granted-set cache's own word: the response's set when it names one, the
requested set when it names none.

**The surfaces.** A standing refusal state is snapshot data, not an event:
an additive `grant` field on the settings snapshot's account rows, sourced
through the host's account-info seam from a map the boot dial and the
sign-in's completion write. The completion event stays the wizard's
one-shot channel; a surface that mounts late, or a relaunch, must pull
standing state, never catch an emission. What the writers record: the boot
dial classifies a dead grant into the expired-sign-in state for the
account it then skips (the row is the only surface explaining the absence,
and it renders even though the app holds no such account, because the row
is the only surface offering the re-grant); a dialled calendar probe that
answered reauthorization records the withheld calendar; a healthy dial
clears whatever stood, because a dial that answered proves the standing
state wrong; a refused completion records nothing, because its rollback
rule leaves everything that stood exactly as it was. The reconnect banner
(a withheld calendar) and the expired-sign-in row (a dead grant, never
badged as a server outage) both offer the same one action: run the sign-in
again, targeted at the account's address.

**The scope set, scoped to what shipped.** A Microsoft sign-in asks
`MICROSOFT_GRAPH_SCOPES` (the union of the use groups in the pure half's
scope tables); what the shipped account consumes of it:

| Scope | Use it serves today |
|---|---|
| `offline_access` | the refresh token; the completion's door refuses a token set without one (rule 7) |
| `User.Read` | the `/me` address lookup |
| `Mail.ReadWrite` | mail sync and every mail write; the group's needed scope |
| `Mail.Send` | `POST /me/sendMail`; never granted by the mail-read scope |
| `Calendars.ReadWrite` | the default calendar's reads and writes |
| `Contacts.ReadWrite` | requested at consent; the contacts leg over Graph rides the contacts slice |
| `User.ReadBasic.All` | requested at consent; the colleague directory rides its slice |

A use the grant withholds is not opened: the granted-set cache answers at
launch and offline, and a withheld calendar scope surfaces as the
reconnect state through the dial's `403` probe rather than a crash.

**The offline proof.** The engine's Graph adapter offers no offline
replay (its live tests are gated on a real access token, its fixtures are
recorded bodies for the parsers), and a stored document's token endpoint
is pinned to the real Microsoft authority through its tenant, so the
proof runs every offline-reachable link over scripted loopback servers:
the real `/me` lookup, the real token source's refresh, single-flight and
rotation through the real sink into the real dial table and credential
transaction, the completion's ordering, the boot's stored-account dial,
and the dead grant's classification reaching the accounts row. The dial
seam itself is scripted in that proof; its token boundary is pinned by the
account layer's dial tests. The live proof over the real Graph awaits the
Azure registration, and nothing fakes it.

## What ships today

Legend: ✅ implemented and pinned · 🚧 shipped but runtime-unverified or
partial · ⬜ planned (the consuming slice named).

| Gate | Status | Notes |
|---|:---:|---|
| Authorization Code + PKCE, public client | ✅ | the pure half; form bodies pinned byte for byte |
| System browser, never a WebView | 🚧 | the opener-backed seam ships; no runtime sign-in exists (no registration) |
| Custom-scheme capture, raw URL handed back | 🚧 | the deep-link wiring ships (the boot hook, the Windows argv hand-off); runtime-unverified |
| `state` validated, `iss` mandatory where advertised | ✅ | pinned by the pure half's tests |
| Refresh token only, in the OS keystore | 🚧 | the transaction ships; the Microsoft completion persists accounts, offline-proven (no registration, so no real sign-in has run) |
| A refresh names no scope | ✅ | the refresh body carries no scope parameter, pinned |
| Granted-set cache write rules | ✅ | pinned |
| Refusal classified by code, never text | ✅ | `Dead` / `Underscoped` / `Indeterminate` |
| Transport reach split, `ResponseLost` its own error | ✅ | pinned with a real client |
| Replay-safe token source (single-flight, failure memo, cool-downs) | ✅ | SP2m; one refresh per account, the failure shared with queued callers, rotations through rule 5's transaction |
| The Microsoft account: dial, `/me` lookup, re-consent, door rule | 🚧 | offline-proven end to end over scripted servers (the lifecycle suite); the live proof awaits the Azure registration |
| Microsoft refusal surfaces: standing state on the accounts rows | 🚧 | the reconnect banner and the expired-sign-in row read the boot dial's and the completion's classification; Microsoft family only |
| Discovery fail-closed (HTTPS, issuer match, no redirects, S256) | ✅ | pinned offline, plus the live proof below |
| RFC 7591 dynamic registration | 🚧 | code and tests; unproven against a real server |
| Registrations injected, absent supported | ✅ | env-gated; none shipped today, the routes not offered |
| Abandoned sign-in cancelled (explicit; the supersede shape is a recorded gap), no crossed waiter | ✅ | pinned, including the displaced-waiter hand-back |
| Failed sign-in surfaced with its classification | ✅ | the refusal sentences and the honest success sentence |

## The live discovery proof

The harness (docker/harness) publishes Stalwart's JMAP listener on
`127.0.0.1:2080` as plain HTTP, and the gated live proof
(`astra-oauth`'s `discovery_live` tests, run with `scripts/dev/harness.sh
env`) pins what it actually serves:

- The origin yields a real RFC 9728 document through the well-known
  fallback route (the origin's unauthenticated GET answers a redirect
  with no `WWW-Authenticate` challenge, and the no-follow policy treats
  that as no pointer), naming `https://mail.example.test` as both the
  resource and its authorization server. The first half of the chain runs
  green live.
- Stalwart also serves a real RFC 8414 document claiming that issuer,
  advertising S256 and a registration endpoint. The harness domain
  resolves nowhere by design, so the document cannot be fetched under the
  issuer it names and no green live metadata validation exists. What the
  proof pins instead is the refusal: asking about the loopback origin that
  really served the document fails closed on the RFC 8414 section 3.3
  issuer mismatch, the exact check that stops a compromised resource
  pointing the app at an attacker's token endpoint, exercised against a
  real server's real document. The validated-green half of the chain
  stands on the offline decision tests, which hold every rule a plaintext
  mock would have to disable.

No harness OAuth server exists beyond the metadata documents: real
sign-ins are the provider slices' proofs, against the owner's accounts
once registrations exist.

## Known gaps

- **No registration exists, so nothing user-visible ships, and the live
  proof has not run.** The Microsoft account wiring (SP2m) ships
  offline-proven end to end over scripted servers, but the wizard's OAuth
  offer still renders for no one. The live proof (the real browser
  sign-in, a mail sync, a calendar read and write, the refresh rotation
  against the real endpoint) lands when the owner's Azure registration
  does, and is never faked. If that registration turns out to require a
  client secret, the divergence from the public-client shape is escalated
  in its PR. The other provider slices consume the same floor: SP2n
  (Google native APIs), SP2o (JMAP OAuth, live-provable against the
  harness once it mints grants), SP2v (IMAP/SMTP OAuth).
- **The refusal surfaces ship for the Microsoft family only.** The Google,
  JMAP, and password families wait for their slices; the cross-provider
  spread of the expired-sign-in row is the accounts page's own later
  change.
- **A Microsoft account keeps no server-side draft.** The refreshing
  wrapper deliberately does not advertise the engine's server-side drafts
  capability, so the composer holds a Microsoft draft in the app alone
  until the drafts slice wires Graph's draft folder.
- **The principal is `Me` only.** Shared and other mailboxes defer: one
  account is its own mailbox today.
- **Contacts and the directory are requested but not consumed.** The
  consent screen lists `Contacts.ReadWrite` and `User.ReadBasic.All` (the
  set ships as a whole), while the legs that would use them ride the
  contacts and directory slices.
- **A re-grant targets by `login_hint` alone.** Completing the browser
  flow as a different address adds a second account rather than being
  refused; the wizard-begun flow has the same shape. Pinning the target
  rides the provider slices' completion rework.
- **A grant that dies mid-session is unobserved until a dial runs
  again.** The sync pass swallows per-account failures, so the expired
  row appears at the next launch, not the moment the provider revokes.
- **An offline relaunch classifies nothing.** Only a dial that reaches
  the token endpoint can answer Dead, so the accounts page holds no
  expired row until the network returns; the last classification is not
  persisted (rule 12's "not suppressed while offline" binding).
- **A near-empty grant passes the door.** The door refuses a grant that
  names no scope at all; one naming `offline_access` alone passes, dials,
  and lands in the mail leg's refusal rather than a door sentence.
- **A Graph calendar's display zone is UTC.** The host carries no zone
  setting yet; the settings slice that owns one replaces it.
- **RFC 7591 registration is unproven against a real server.** The
  harness advertises a registration endpoint, but its issuer document
  cannot be reached under its own name and no registration round trip has
  run.
- **The Windows browser is the OS default.** Reusing the person's browser
  session is the point, and the cost is inherited from it: no API closes
  the tab after the custom-scheme hand-off, so a blank tab remains.
- **The Google Desktop loopback redirect is not built.** Google's Desktop
  client type cannot use a custom scheme; its `http://127.0.0.1:<port>/`
  capture listener lands with the Google slice (SP2n). The registration
  schema anticipates it (no redirect travels with the registration).
- **A completed sign-in with no refresh token is not yet refused.** Rule
  7's enforcement point is the account wiring the provider slices land;
  today nothing persists a sign-in at all.
- **One build identity, one scheme.** The reference's packaged-versus-dev
  scheme split (Windows registers a protocol per user, so two builds
  claiming one scheme put a picker in front of a live auth code) has no
  analogue while only one build identity exists; rule 8 records the
  revisit.
