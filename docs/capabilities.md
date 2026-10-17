# Capabilities

What Astra ships, per area. A row moves from ⬜ to 🚧 to ✅ only in the same
change as the feature it describes (rulebook: Contract changes). Rows marked
N/A carry their reason in Notes. Source links the mail-calendar contract
this row ports.

| Area | Feature | Status | Source |
|---|---|---|---|
| Accounts | IMAP/SMTP account setup wizard with server discovery and manual server entry | 🚧 | accounts.md |
| Accounts | Auto-detect from email address (JMAP probe, SRV, autoconfig, ISPDB, MX), with results from an unencrypted hop disclosed and needing explicit approval | 🚧 | account-autodetect.md |
| Mail | Folder pane across accounts with nested folders and persisted expansion | 🚧 | folder-pane.md |
| Mail | Flat and threaded message list with windowed paging | 🚧 | mail-calendar capabilities |
| Calendar | Day, work week, and month grids with drag to create, move, resize | 🚧 | grids, agenda, drag move, and drag-create across empty time ship, the create landing in the dialog prefilled; resize is the dialog's end edit; the host dials the calendar leg at boot when the account carries a dav section; calendar.md |
| Core | Reactive Rust to native binding, dispatch to immutable snapshots | 🚧 | |
| Accounts | Settings accounts page lists every account, with per-service toggles, relinking, sign-in again, server editing, and removal | 🚧 | the page lists every account with its dialled legs, the dav section editor, and a confirmed removal; no per-service toggles, relinking, sign-in again, or server editing; settings.md |
| Accounts | Outgoing name per account, asked once at connect, editable in settings, kept in step with the provider | ⬜ | sending.md |
| Accounts | Untrusted server certificates shown with a fingerprint and accepted once per server; verification is never switched off | 🚧 | certificate-exceptions.md |
| Accounts | OAuth sign-in foundation: the browser PKCE client, the credential transaction, standards discovery, and the refusal classification | 🚧 | the pure client, the host sign-in session and credential transaction, the env-gated registrations (none shipped, so no OAuth route is offered), and the live discovery proof against the harness all ship; the Microsoft and Google accounts consume the floor (offline-proven end to end over scripted servers); provider-oauth.md |
| Accounts | Microsoft 365 accounts: browser OAuth sign-in (PKCE), mail read and actions, send, calendar read and write | 🚧 | the whole lifecycle is offline-proven over scripted servers (the sign-in through the real `/me` lookup and token refresh, the rotation landing in the store, the boot re-dial, the dead grant's row); the principal is `Me` only (shared mailboxes defer) and a Microsoft account keeps no server-side draft; the calendar's display zone is UTC; the live proof awaits the Azure registration; provider-oauth.md |
| Accounts | Microsoft reconnect banner when a permission is missing; one tap re-grants the full scope set | 🚧 | the withheld calendar (the 403 probe's reauth answer) raises the banner on the accounts page and a per-account badge on the folder pane; one tap re-runs the sign-in targeted at that account's `login_hint` (targets, does not pin: completing as another address adds a second account); the standing state rides the settings snapshot's account rows; live proof awaits the Azure registration; provider-oauth.md |
| Accounts | Expired or revoked sign-ins reported as such with an offer to sign in again, across Microsoft, Google, JMAP, and password accounts | 🚧 | the Microsoft Graph, Google, and JMAP halves ship: a dead grant at the boot dial renders the expired-sign-in row on the accounts page (never an outage badge), its action the same re-grant over the account's own provider for the integrated families, while a JMAP row names the refused credential and its remedy is reconnecting the account (its own in-place re-grant waits for its slice); a grant that dies mid-session is unobserved until the next dial, and an offline relaunch classifies nothing until the token endpoint answers; the password family waits for its slice; provider-oauth.md |
| Accounts | Google accounts over native APIs: browser OAuth sign-in (PKCE), mail read and actions, send, calendar | 🚧 | the whole lifecycle is offline-proven over scripted servers (the loopback sign-in's completion through the real granted-set userinfo lookup and token refresh, the rotation backstop landing in the store, the boot re-dial, the dead grant's row over the google kind); the account type appears only in a build carrying the Google Cloud registration, and the live proof awaits it; the sender-name write and the contacts leg wait for their slices; provider-oauth.md |
| Accounts | JMAP accounts (RFC 8620/8621): one secret, mail read with bodies, send, calendar read | 🚧 | the account connects end to end offline: the wizard's JMAP form (one secret field, the derived URL editable), the host-side document derivation and add, the boot dial for the password and grant shapes, the account-wide folder binder, and the accounts-page kind row all ship, the password and OAuth legs proven over scripted servers; the live proof against a real JMAP server and the calendar read ride their slice; jmap.md |
| Accounts | JMAP OAuth sign-in discovered entirely from the standards, with no per-provider code | 🚧 | the discovery chain (RFC 9728 to 8414 to 7591) and the persisted `[jmap.oauth]` grant ship, and the wizard offers the sign-in only where the server's own 401 advertises it (fail soft, the password field always standing), with the completion dialling over the refreshing wrapper and landing rotations through the sink; proven offline over scripted servers, the live proof against a real server rides its slice; jmap.md |
| Accounts | IMAP/SMTP sign in with your provider where the server offers it, discovered from the standards | ⬜ | mail-oauth.md |
| Accounts | JMAP sign-in renewed in place, keeping mail, folders, and settings | ⬜ | jmap.md |
| Accounts | Multi-account with unified inbox and account switcher | ⬜ | |
| Mail | Unified mail as an All Accounts group in the folder pane, remembered across launches | ⬜ | folder-pane.md |
| Accounts | Per-account credential storage with in-app account removal | 🚧 | each account's config document sits in the host's credential store; the accounts page's confirmed removal deletes it; settings.md |
| Accounts | Account list offered on the next device from a vendor account: address and servers, never a password | ⬜ | onboarding.md |
| Accounts | Vendor account sign-in, creation, management, and deletion in its own settings category | ⬜ | allodia_license/entitlement.md |
| Accounts | Subscription view: charges, renewal, retried payment, multiple-biller warning, cancel and restart under direct billing | ⬜ | allodia_license/purchasing.md |
| Mail | Create, rename, move, and delete folders from the folder pane; queued offline; delete goes to Trash first | ⬜ | folder-pane.md |
| Mail | Drag a folder into another folder or onto its account; drag messages onto a folder | ⬜ | folder-pane.md |
| Mail | Move to folder action in the message row menu, within one account | ⬜ | folder-pane.md |
| Mail | Threaded conversation view: the whole conversation including your sent replies, across folders | 🚧 | mail-calendar capabilities |
| Mail | Reading view with sanitised HTML, inline CID images, recipient headers, remote-image gating, and retry | 🚧 | |
| Mail | Web and mail addresses in message text open in the browser or mail app; http, https, and mailto only | 🚧 | rendering-security.md |
| Mail | Message laid out against the reading pane's own width, so a newsletter's own mobile rules apply | ⬜ | reading-zoom.md |
| Mail | Fixed-width messages reflowed to fit at every pane width, live during resize | ⬜ | reading-zoom.md |
| Mail | Offline-first bodies: synced bodies auto-download after every sync under a per-account cap, so opens are instant | ⬜ | |
| Mail | Three-pane desktop layout: resizable folder, list, and reading pane cards, app navigation on a left icon rail, message actions on a command ribbon under the title bar | 🚧 | |
| Mail | Reading actions: reply, reply all, forward, archive, delete | 🚧 | sending.md |
| Mail | Overflow menu with Save as .eml, writing the message exactly as delivered | ⬜ | reading-actions.md |
| Mail | Print a message with subject and header lines through the platform print dialog | ⬜ | reading-actions.md |
| Mail | Attachments: list, save, and open received files; attach files in the composer | ⬜ | |
| Composer | Rich HTML composer for new mail, reply, reply all, and forward, with editable To, Cc, and Bcc | 🚧 | the composer opens in its own window; a quick reply docks under the reading pane |
| Composer | A forward opens holding the original's attachments, removable one by one | ⬜ | sending.md |
| Composer | Drafts stored on the server while you type; one draft across devices; taken away on send | 🚧 | drafts.md |
| Composer | A Drafts row opens back into the composer and saves over the same copy | ⬜ | |
| Mail | Open a message in its own window carrying the whole reading view | ⬜ | reading-window.md |
| Composer | From account picker with an app-level default send account | ⬜ | |
| Composer | Formatting: bold, italic, underline, font size, text and highlight colour, nested lists, tables with row and column editing | ⬜ | |
| Composer | Links made, changed, or removed on the selection; typed addresses become links; http, https, and mailto only, checked on send | ⬜ | composer-security.md |
| Composer | Composer toolbar chrome in the app's language, from the shared catalog | ⬜ | composer-security.md |
| Composer | Subject editable in replies and forwards, with derived Re: and Fwd: prefixes | 🚧 | sending.md |
| Composer | Paste a picture into the message or a signature, sent as an inline cid: part | ⬜ | composer-security.md |
| Composer | Resize a pasted or quoted picture by dragging a corner; ratio kept | ⬜ | composer-security.md |
| Composer | Drag files onto the composer: a document attaches, a picture asks inline or file | ⬜ | composer-security.md |
| Composer | mailto: links open the composer prefilled; extra headers dropped | ⬜ | composer-security.md |
| Composer | Share files into a new message from the OS share sheet; unsafe names and types dropped | ⬜ | os-integration.md |
| Platform | Offer to become the default mail app, asked once, changeable in settings | ⬜ | os-integration.md |
| Composer | Reply and forward quoting below the original, app default with per-message override | 🚧 | sending.md |
| Composer | Signature library with rich text and an inline logo; per-account for new mail and replies; auto-swapped with From | ⬜ | signatures.md |
| Mail | Mail actions: read and unread, flag, archive, delete, trash | ⬜ | |
| Mail | Select several messages and act on the lot: archive, trash, delete, read, unread, flag | ⬜ | list-selection.md |
| Mail | Mark as spam or not spam, reported to the provider so its filter learns | ⬜ | reporting.md |
| MCP | MCP server over a local socket, opt-in and off by default: read and act on mail from an MCP client; no send verb to the agent, ever | 🚧 | the server, the account gate, and the Advanced settings panel ship (host-side state: off and empty at every boot, nothing persists); no send tool exists at all, the recorded cut; the end-to-end and live proofs land over the built relay against the harness; the relay bundle beside the app remains until the packaging pass; mcp.md |
| Mail | Configurable swipe actions per direction, with undo | ⬜ | |
| Calendar | Agenda view with create and delete event | 🚧 | calendar.md |
| Calendar | Write affordances gated on per-row can_write | 🚧 | calendar.md |
| Calendar | Write-status feedback: spinner while saving, warning on unconfirmed, retry | 🚧 | writing and failed-with-retry states settle per dispatch; calendar.md |
| Calendar | All-day and multi-day banners with a per-day overflow count that expands | ⬜ | the all-day lane ships; the banner shape does not |
| Calendar | Calendar manager: per-calendar visibility and colour override, persisted | 🚧 | visibility toggles ship; no colour override, nothing persisted; calendar.md |
| Settings | Display settings: first day of week, 12 or 24-hour clock, light or dark appearance, calendar horizon | 🚧 | all four ship in the settings screen; accent skin, contrast, and font size ship as device-local options; the time zone carries via the calendar's display zone; settings.md |
| Calendar | The shape and horizon you left the calendar in, restored on launch | ⬜ | |
| Calendar | The diary on screen from the store at launch, filled without opening it, there offline | ⬜ | |
| Calendar | Calendar refreshes periodically while the app stays open | ⬜ | |
| Calendar | Event creation with a calendar picker, all-day, notes, location | 🚧 | calendar.md |
| Calendar | Tap an event to view its detail and edit title, time, notes, location | 🚧 | a tap selects the event into the detail dock; the editor opens from there; calendar.md |
| Calendar | Attendee list with responses, on the detail and read-only in the editor | ⬜ | |
| Calendar | Deleting one occurrence of a repeating event asks this event or all events first | 🚧 | calendar.md |
| Calendar | Editing a whole repeating event warns where the server would throw away single-occurrence changes | 🚧 | calendar.md |
| Calendar | A repeating rule read back in full on the event detail | ⬜ | the read-back is minimal; calendar.md's gap |
| Calendar | One occurrence of a repeating event shows that occurrence's date and time | 🚧 | calendar.md |
| Calendar | Editing one occurrence of a repeating event asks this event or all events first | 🚧 | calendar.md |
| Calendar | Setting a repeat rule: how often, how many periods to skip, weekdays, what ends it; a rule too detailed to state is shown, not offered | 🚧 | create path only; edit-time restructuring is calendar.md's gap; calendar.md |
| Invitations | An invitation email shows a card: who is asking, when, where, how everyone answered, conflicts, that day's grid | 🚧 | kind, participation, title, organiser; no roll-call, conflicts, or day grid; invitations.md |
| Invitations | The invitation's own .ics part is not shown as a junk attachment; a really attached calendar file still is | ⬜ | |
| Invitations | An unanswered meeting drawn as a hold on the grid, month, and agenda | 🚧 | invitations.md |
| Invitations | A declined meeting leaves the calendar on every provider; the invitation email is the way back | 🚧 | invitations.md |
| Invitations | Answer accept, maybe, or decline, with a note to the organiser where the account supports it | 🚧 | invitations.md |
| Invitations | An email-only invitation is put on the calendar when answered, and the reply emailed to the organiser where the server will not send it | 🚧 | invitations.md |
| Invitations | A superseded invitation says it is out of date and stops offering an answer | 🚧 | invitations.md |
| Invitations | When the server cannot pass your answer on, you are told and offered email instead, rememberable per account | 🚧 | invitations.md |
| Contacts | Unified contact list across account address books (CardDAV, JMAP, Google, Microsoft), with search and detail | 🚧 | the People view ships with search matched in the engine and the fold, monogram, and section letters derived once in the rebuild; the CardDAV leg dials at boot and is proven live; the JMAP, Google, and Microsoft legs do not stand; contacts.md |
| Contacts | Add and edit a contact in a writable address book; one source card named when a person is filed twice | 🚧 | create and edit land through the engine's contact writes and read back live; the write states settle per dispatch; no delete; contacts.md |
| Sending | Outbox keeps a message that could not be sent and sends it later by itself | 🚧 | sending.md |
| Sending | Outbox actions: send now, cancel, or edit a message that has not gone | 🚧 | the queue lives behind a per-account Outbox folder row and renders in the message list; sending.md |
| Sending | A send interrupted by app close is kept and never sent twice; the user answers Mark as Sent or Send Again | 🚧 | sending.md |
| Sending | A message the server refused stays until sent again, edited, or discarded | 🚧 | sending.md |
| Sending | Reply and reply all from the reading pane: prefilled recipients, subject, and quoted original, sent as a threaded reply | 🚧 | sending.md |
| Sending | Forward from the reading pane: quoted original under a Fwd: subject, to recipients you name | 🚧 | sending.md |
| Sending | Threading written on send: replies link by In-Reply-To with the References chain; forwards carry the chain alone | 🚧 | sending.md |
| Contacts | Sender avatars: a coloured monogram, replaced by an address-book photo | 🚧 | the monogram ships on the People view and the composer's suggestions, one derivation; the photo reads through the validated cached-photo command; no avatar on the mail list row yet; avatars.md, contacts.md |
| Contacts | The same person in several accounts is one row, joined on a shared address, and says so | 🚧 | joined on the shared canonical address, names never join; In N accounts on the row, Also in and per-value provenance on the detail; proven live; contacts.md |
| Composer | Recipient autosuggest ranked from synced contacts and from people you have written to | 🚧 | the token ranking with pills over the engine's suggestions; sent-mail history mined during sync and never hidden; contacts.md |
| Search | from: and to: search DSL, unified across accounts, newest first, every folder but Trash | 🚧 | search.md |
| Search | Search scope filter: narrow to the current folder and back to all mail | 🚧 | search.md |
| Search | Search says how far back it looked, with a link to change the depth | 🚧 | search.md |
| Search | Search results follow the list's conversation grouping | 🚧 | search.md |
| Sync | Per-account fetch depth and message size, with every folder synced within the depth | ⬜ | |
| Sync | Sync progress: a bar under the list for a started download, a status line for background passes, Sync paused when throttled | 🚧 | sync-progress.md |
| Settings | Unified settings screen: language, appearance, time zone, conversation grouping, swipe actions, per-account fetch, quote style, default send account, reset, About | 🚧 | seven categories ship in a modal with a tab rail: General (appearance, clock), Calendar (zone, first day, horizon, default calendar), Reading (grouping), Composing (quote style), Shortcuts, Accounts, About; language, swipe actions, per-account fetch, default send account, and reset stay absent; settings.md |
| Sync | Per-mailbox sync behaviour: IMAP IDLE push where capable, otherwise 15 to 120 minute polling | ⬜ | |
| Sync | Background mail delivery while the app is not in the foreground | ⬜ | background-sync.md |
| Sync | New-mail notifications from the background sync: who it is from, what it is about, how it begins | ⬜ | |
| Sync | Auto-reconnect after network loss, offline banner, per-account outage badge | ⬜ | |
| Settings | Time-zone-aware display with a device-zone change prompt | ⬜ | |
| Settings | Localisation: the app language drives dates as well as copy | ⬜ | timestamps.md |
| Diagnostics | Rotating privacy-safe file log, attachable for support | ⬜ | |
| Diagnostics | In-app diagnostics: log viewer, share and export with a privacy note, persisted debug toggle | ⬜ | logging.md |
| Analytics | Consented product analytics: opt-in and default off, no mail content in the payload, one-click withdrawal | ⬜ | analytics.md |
| Platform | Showcase mode: boots a seeded in-memory dataset for store screenshots | ⬜ | debugging.md |

## Astra: plugins

| Feature | Status | Notes |
|---|---|---|
| Plugin host with manifest, lifecycle, supervision | ⬜ | SP1 |
| Versioned hook API v1 (compose.transform, message.decode) | ⬜ | SP1 skeleton |
| example-plugin e2e | ⬜ | SP1 |
| SMIME plugin package | ⬜ | SP-CRYPTO, post parity |
| OpenPGP plugin package | ⬜ | SP-CRYPTO, post parity |

## Astra: CLI

| Feature | Status | Notes |
|---|---|---|
| Read verbs with --json contract | 🚧 | twenty-six verbs ship: accounts, folders, messages, rules, the stored AI reads (cold, results, drafts, summary, style, digest, report), and the knowledge and snippet reads; grows with SP2+ slices; cli.md |
| Gated mutating verbs with --preview | 🚧 | drafts save and discard, rules create and delete, knowledge forget, snippets create and delete; the model-class spellings (`ai style rebuild`, `ai digest generate`, `ai report generate`, `knowledge extract`, `snippets search`) parse and refuse; cli.md |
| Every invocation audited with a correlation id | 🚧 | ai.sqlite ledger; cli.md |
| Send-class verbs parsed but refused | 🚧 | exit 3; cli.md |
| Virtual permission tools mapping | ⬜ | SPA0 |
| Shell completions | ⬜ | SP-CLI |

## Astra: AI foundation (SPA0)

| Feature | Status | Notes |
|---|---|---|
| Tier and use-case registry from `ASTRA_LLM_*` env (chains, per-tier fallback, protocol per provider prefix) | 🚧 | the SPA0 set plus the judge family, five tiers; ai-foundation.md |
| Background LLM client: dual protocol, retryable chain fallback, quota and auth terminal | 🚧 | one ledger row per attempt; consumed by the A1 incoming pass and its seams; ai-foundation.md |
| Product AI ledger (`llm_calls`, `audit_log`) beside the engine store | 🚧 | ai.sqlite, shared by host and CLI; rusqlite per the recorded decision; ai-foundation.md |
| Untrusted-content wrapper and DLP policy types | 🚧 | active in A1: allow, redact, block from `ASTRA_DLP`; ai-rules.md |
| Binary judge: strict single-token verdicts, retry once on abstain, safe-default table, nano spend guard | 🚧 | verdicts in the ledger row outcome; budget from `ASTRA_LLM_NANO_BUDGET_MICROS`; consumed by A1's stages; binary judge spec |
| Eval harness: scenario fixtures, offline battery in CI, live judge scenarios under the gate | 🚧 | partial: offline battery and three live scenarios ship; the live set grows as eval content; ai-rules.md |
| Offline agent loop e2e: prompt, gated approval, tool run, one correlation id across both ledgers | 🚧 | real sidecar bundle plus the stub CLI, in the desktop test suite; ai-foundation.md |

## Astra: AI (inbox-zero port, phases per spec 9.3)

| Phase | Feature | Status | Notes |
|---|---|---|---|
| A1 | AI rules: prompt-to-rules, matching, deterministic execution | 🚧 | ai-rules.md |
| A1 | Sender categorisation | 🚧 | ai-rules.md |
| A1 | Cold email blocker | 🚧 | recorded verdicts only; nothing executes on them in A1; ai-rules.md |
| A2 | Reply drafting with writing style | 🚧 | draft pool, style profiles, the reading pane arming; ai-replies.md |
| A2 | Smart reply chips: gated surfacing, accept into the reply composer, dismissal | 🚧 | the gate and chips ship; no badge surface on categorisation yet; ai-replies.md |
| A2 | Summarisation with cache | 🚧 | the reading pane's summarise block; ai-replies.md |
| A2 | Assistant chat with tool budget | ⬜ |
| A3 | Knowledge: batch extraction with sources, draft-prompt shaping, forget | 🚧 | ai-assist.md |
| A3 | Snippet library with ranked search and the composer picker | 🚧 | ai-assist.md |
| A3 | Compose autocomplete: gesture-only chip with session dismiss | 🚧 | ai-assist.md |
| A3 | Reading-pane translate into six languages, cached | 🚧 | ai-assist.md |
| A4 | On-demand digest and analytics report with the AI documents panel | 🚧 | ai-digest.md |
| A4 | Split inbox (All / Actioned / FYI) from the stored categorisation | 🚧 | ai-digest.md |
| A4 | Meeting briefs | 🚧 | core and live relayed proof ship; the host dials the calendar leg at boot when the account carries a dav section; ai-digest.md |

## Astra: Agent (sidecar host, per SPA0 spec D5)

| Feature | Status | Notes |
|---|---|---|
| Node sidecar hosted with supervision (ready handshake, linear capped backoff, Lost after budget) | 🚧 | desktop; node >= 22 from PATH or ASTRA_AGENT_NODE; sidecar dist bundled as a resource |
| Permission routing: asks surface as approval cards, answers ride back, everything else fails closed | 🚧 | desktop; every decision audited with the turn's correlation id |
| Agent chat panel: streaming log, prompt, abort | 🚧 | desktop; llm_call rows land in the ai.sqlite ledger |
| Correlation id end to end: prompt to tool spawns to both ledgers | 🚧 | generated by the host per turn when the caller brings none |
