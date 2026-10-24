// SPDX-License-Identifier: GPL-3.0-only
//! Gated live tests against the local Stalwart harness (docker/harness): the
//! product's IMAP connect path, over real TLS, against a server whose certificate
//! nobody trusts; and the product's detection path, over the real HTTP fetcher,
//! against a server whose domain resolves nowhere.
//!
//! The harness serves implicit TLS on `127.0.0.1:2143` with a self-signed
//! certificate, so the first dial is refused as a certificate rejection and
//! the test pins the accepted certificate exactly as the product's owner
//! would: a [`CertificateException`] recorded for that one server. That is
//! the recorded TLS route for this harness (the decision sits in
//! `docker/harness/compose.yaml` and `scripts/dev/harness.sh`): the account
//! layer dials TLS only, and the same certificate serves the SMTP submission
//! listener (`127.0.0.1:2025`), both legs naming the one server, so the one
//! accepted exception covers both dials.
//!
//! The seed script (`docker/harness/stalwart/seed.sh`) also delivers three pinned
//! notes to astra1's INBOX through the harness's own SMTP submission port (implicit
//! TLS; curl's `smtps` with `-k`, inside the container, accepting the same self-signed
//! certificate the IMAP seeding does). The notes are genuinely submitted and
//! delivered: Stalwart runs them through its delivery path into the INBOX, exactly the
//! mail the gated list test reads back through the product's own dial and sync. The
//! same seed files one rich MIME note into Projects (submission into the INBOX, then
//! an IMAP move), the message the gated reading test opens in `reading`. The gated
//! drafts test needs no seed: it round-trips the drafts family against the account's
//! real Drafts folder through the product's own dispatches. The gated send test
//! (`send`) needs no seed either: it submits its own mail from astra1 to astra2
//! through the engine's real send path, reads the delivery back, and withdraws both
//! copies so the harness keeps its seeded shape; the attachment proof in the same
//! file carries the wire one file further, staging one attachment behind a minted
//! handle whose bytes the app-level read returns exactly. The gated reply and
//! forward tests (`reply`, `forward`) walk the answer chain over the same wire:
//! each first submits a fresh note from astra1 to astra2 (the seed leaves astra2
//! nothing to answer), then composes the answer through the product's own compose
//! pipeline, submits it as a reply or a forward, and reads the delivery back at
//! astra1 with the threading pinned (`In-Reply-To` matching the original's
//! `Message-ID` for the reply; `References` only, under a `Fwd:` subject, for the
//! forward). All of them withdraw every copy they made, so astra1's INBOX ends the
//! run holding the three seeded notes again and astra2 holds nothing. The gated
//! folder tests (`folders`, `move_to_folder`) walk the pane's folder writes end to
//! end over the IMAP
//! dial: the folder lifecycle (created nested, renamed under the receipt's minted
//! key, reparented, trashed, deleted for good) and a move-to-folder batch, each
//! run withdrawing what it created and rescuing the seeded notes from inside it.
//! The gated unified proof (`unified`) lists both harness accounts' inbox rows on
//! the all-inboxes scope, newest first, over a fresh note submitted across the
//! same wire. The gated search
//! test (`search`) reads rather than writes: it needs no seed of its own and no
//! cleanup, because every query it runs crosses the engine's real search surface over
//! the corpus the seed already pinned (the rich note sits in Projects, so the
//! `from:` hit proves the answer crosses folders), and the harness keeps its seeded
//! shape whatever the queries find. The gated people
//! tests (`people`) extend the same discipline to the CardDAV face: cards are
//! seeded onto each account's default address book through a raw helper under
//! unique run tokens, the dial proof runs the real `connect_dav_providers`,
//! every assertion reads through the product's own paths, and the autosuggest
//! proof submits its own mail so the history it ranks is the run's
//! own, and every seeded card and sent mail is withdrawn at the end (cards
//! through the raw seeding helper: the product ships no contact delete; mail
//! through the product's own delete). The gated settings tests (`settings`)
//! prove the settings editor's account seam live: the dav editor's save dials
//! the real `connect_dav_providers` over a scratch config mirroring the
//! store's merge rule and dial-before-persist ordering, installs both legs,
//! keeps what stood when a re-dial fails, and tears down on the clear; the
//! zone proof pins the now line's zone id. The gated composer proofs
//! (`composer_proof`) carry the composer's finishing surfaces over the same
//! wire: a formatted send (a font size, a colour, and a highlight run) round
//! trips with its inline CSS intact, a signature-bearing send round trips
//! both legs (the RFC 3676 delimiter in the plain, the signature div in the
//! html), and a link round trips both ways (a good https target arriving as
//! an anchor, a javascript: target in the sent wire degrading to plain
//! text), each proof withdrawing both copies like every delivery proof.
//! The gated delivery proofs (`delivery_proof`) pin the background loop's
//! landings over the same wire: the recipient's INBOX watch, opened
//! through the account layer's own `open_watch`, answers `Changed` for a
//! mail submitted while the watch stands, the account's ordinary pass
//! lands it, and the core's new-mail queue holds its notice; the poll
//! arm's pass lands a delivered mail through the same seam beside the
//! cadence the settings carry. The gated warm proof (`warm_proof`) runs
//! the per-account body cap's states over the account's own passes: the
//! off account as metadata alone, the byte cap's pass warming until the
//! missing-body read empties, and the return to off dropping the cached
//! sources with the reading pipeline still answering, the opened note put
//! back to unread at the end. The gated outage proof (`outage_proof`)
//! proves the classification over a real refused dial: a config pointing
//! at a released loopback port (the shared harness is never stopped), two
//! refused passes putting the only dialled account in outage with the
//! banner answering, and the real dial's own successful pass clearing
//! both.
//!
//! The gated JMAP tests (`jmap`) prove the password account end to end over
//! the harness's plaintext JMAP origin: the dial's session snapshot, the
//! folder list, the seeded notes, a body read, a send round trip with both
//! copies withdrawn, the calendar read the session advertises (seeded over
//! CalDAV, read back over JMAP), and the OAuth gate's fail-soft refusal.
//!
//! The JMAP-detection test cannot dial `https://example.test/.well-known/jmap` (the
//! harness domain resolves nowhere), so it rebases the probe onto the harness's plain
//! HTTP JMAP origin (`127.0.0.1:2080`) through the detection crate's dev-harness
//! `well_known_base_override`, mirroring mail-calendar's live JMAP tests; no
//! host trust store is touched.
//!
//! The tests skip unless `ASTRA_LIVE_HARNESS=1`, so the offline `cargo test` stays
//! green. Against a running harness:
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! cargo test -p astra-app tests_live_harness -- --nocapture --test-threads=1
//! ```
//!
//! The suites share one live server whose contracts pin whole states (the
//! list suite's three-note INBOX; the reply and forward suites deliver into
//! that INBOX before withdrawing their copies), so the recorded invocation
//! runs them one at a time.

use std::sync::atomic::{AtomicUsize, Ordering};

use engine_api::{AccountId, Provider};

use astra_account::{
    connect_mail_providers, AccountConfig, AccountError, AccountShape, CertificateException,
    ConnectionSecurity, ImapAccount, ImapConnections, Secret, SmtpAccount,
};
use astra_viewmodel::{FolderRole, MailboxListSnapshot, MessageRowView, RowKey};

use crate::{reference::MessageRef, App, AppObserver, FolderRef, Intent, RowRef, Surface};

// The gated suites live in their own files, so this one stays under the
// 500-line cap the gate enforces; the explicit `#[path]`s because this file
// owns no directory a bare `mod` would resolve into. `proof` holds the
// fixtures the delivery proofs share; `folder_proof` the folder proofs'.
#[path = "tests_live_harness/calendar.rs"]
mod calendar;
#[path = "tests_live_harness/calendar_brief.rs"]
mod calendar_brief;
#[path = "tests_live_harness/calendar_invite.rs"]
mod calendar_invite;
#[path = "tests_live_harness/calendar_proof.rs"]
mod calendar_proof;
#[path = "tests_live_harness/composer_proof.rs"]
mod composer_proof;
#[path = "tests_live_harness/crypto_send.rs"]
mod crypto_send;
#[path = "tests_live_harness/delivery_proof.rs"]
mod delivery_proof;
#[path = "tests_live_harness/drafts.rs"]
mod drafts;
#[path = "tests_live_harness/folder_proof.rs"]
mod folder_proof;
#[path = "tests_live_harness/folders.rs"]
mod folders;
#[path = "tests_live_harness/forward.rs"]
mod forward;
#[path = "tests_live_harness/gate.rs"]
mod gate;
#[path = "tests_live_harness/jmap.rs"]
mod jmap;
#[path = "tests_live_harness/list.rs"]
mod list;
#[path = "tests_live_harness/move_to_folder.rs"]
mod move_to_folder;
#[path = "tests_live_harness/outage_proof.rs"]
mod outage_proof;
#[path = "tests_live_harness/people.rs"]
mod people;
#[path = "tests_live_harness/people_proof.rs"]
mod people_proof;
#[path = "tests_live_harness/proof.rs"]
mod proof;
#[path = "tests_live_harness/reading.rs"]
mod reading;
#[path = "tests_live_harness/reply.rs"]
mod reply;
#[path = "tests_live_harness/search.rs"]
mod search;
#[path = "tests_live_harness/send.rs"]
mod send;
#[path = "tests_live_harness/settings.rs"]
mod settings;
#[path = "tests_live_harness/settings_proof.rs"]
mod settings_proof;
#[path = "tests_live_harness/unified.rs"]
mod unified;
#[path = "tests_live_harness/warm_proof.rs"]
mod warm_proof;

/// The harness connection details from the environment, or `None` to skip.
fn harness() -> Option<(String, String, String, String)> {
    if std::env::var("ASTRA_LIVE_HARNESS").ok().as_deref() != Some("1") {
        return None;
    }
    let host = std::env::var("ASTRA_HARNESS_IMAP_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let port = std::env::var("ASTRA_HARNESS_IMAP_PORT").unwrap_or_else(|_| "2143".to_owned());
    let user = std::env::var("ASTRA_HARNESS_IMAP_USER")
        .unwrap_or_else(|_| "astra1@example.test".to_owned());
    let password =
        std::env::var("ASTRA_HARNESS_IMAP_PASSWORD").unwrap_or_else(|_| "astra-harness".to_owned());
    Some((host, port, user, password))
}

/// The harness's plain-HTTP JMAP origin from the environment, or `None` to skip.
fn jmap_base() -> Option<String> {
    if std::env::var("ASTRA_LIVE_HARNESS").ok().as_deref() != Some("1") {
        return None;
    }
    let url = std::env::var("ASTRA_HARNESS_JMAP_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:2080".to_owned());
    Some(url)
}

/// The harness's SMTP submission port from the environment: the compose file publishes
/// Stalwart's implicit-TLS submission listener on `127.0.0.1:2025`.
fn harness_smtp_port() -> String {
    std::env::var("ASTRA_HARNESS_SMTP_PORT").unwrap_or_else(|_| "2025".to_owned())
}

/// The harness account, with the certificate exceptions its owner has accepted (none
/// for the first dial). Both legs name the same server, so one accepted certificate
/// covers the IMAP dial and the SMTP submission alike.
fn account(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
    exceptions: Vec<CertificateException>,
) -> AccountConfig {
    AccountConfig {
        imap: Some(ImapAccount {
            addr: format!("{host}:{port}"),
            server_name: host.to_owned(),
            username: user.to_owned(),
            password: Some(Secret::new(password.to_owned())),
            security: ConnectionSecurity::ImplicitTls,
        }),
        smtp: Some(SmtpAccount {
            addr: format!("{host}:{}", harness_smtp_port()),
            server_name: host.to_owned(),
            security: ConnectionSecurity::ImplicitTls,
        }),
        certificate_exceptions: exceptions,
        graph: None,
        google: None,
        jmap: None,
        crypto: None,
        dav: None,
        shape: AccountShape::default(),
    }
}

/// Dials the harness account through the certificate acceptance path a person's first
/// connect walks: the self-signed certificate is refused, the rejection is recorded,
/// the one certificate is accepted for the one server, and the re-dial returns the
/// account id, every provider it bound (the INBOX first), the accepted config, and
/// the shared connections: a late folder bind (the connector a folder proof
/// installs) draws on the same account without a second handshake.
async fn dial_with_accepted_certificate(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (
    AccountId,
    Vec<Box<dyn Provider>>,
    astra_account::AccountConfig,
    std::sync::Arc<ImapConnections>,
) {
    let config = account(host, port, user, password, Vec::new());
    let account_id = config.account_id().expect("the account id");
    let connections = ImapConnections::new();

    let Err(error) = connect_mail_providers(&connections, &config, &account_id, None).await else {
        panic!("the harness certificate was trusted without an accepted exception");
    };
    let AccountError::CertificateRejected { rejected, .. } = error else {
        panic!("the self-signed certificate was not offered as a rejection: {error}");
    };

    let accepted = CertificateException::new(rejected.server_name(), rejected.certificate());
    let config = account(host, port, user, password, vec![accepted]);
    let providers = connect_mail_providers(&connections, &config, &account_id, None)
        .await
        .expect("the dial with the accepted certificate");
    (account_id, providers, config, connections)
}

/// The three subjects the harness seed pins; the list test's fixtures.
const NOTES: [&str; 3] = ["Harness note one", "Harness note two", "Harness note three"];

/// Counts the surface signals the app fires, so the live list test can assert the loop
/// republished the list a write flipped.
#[derive(Default)]
struct CountingObserver {
    /// How many surface changes have been signalled, across every surface.
    signals: AtomicUsize,
}

impl CountingObserver {
    /// The count so far.
    fn signals(&self) -> usize {
        self.signals.load(Ordering::SeqCst)
    }
}

impl AppObserver for CountingObserver {
    fn surface_changed(&self, _surface: Surface) {
        self.signals.fetch_add(1, Ordering::SeqCst);
    }
}

/// The INBOX's folder key as the pane projects it for `account`.
fn inbox_key(app: &App<Box<dyn Provider>>, account: &AccountId) -> String {
    app.mailbox_list()
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .and_then(|row| {
            row.folders
                .iter()
                .find(|folder| folder.role == Some(FolderRole::Inbox))
        })
        .map(|folder| folder.key.as_str().to_owned())
        .expect("the pane lists the account's INBOX")
}

/// The one list row whose subject matches, the list tests' fixture assertion.
fn row_by_subject<'a>(snapshot: &'a MailboxListSnapshot, subject: &str) -> &'a MessageRowView {
    snapshot
        .rows
        .iter()
        .find(|row| row.subject == subject)
        .unwrap_or_else(|| panic!("no row with subject {subject}"))
}

/// Flips `subject`'s row to `read`, building the action's reference from the row the
/// list actually shows: the row's own key is the ref an action carries.
async fn mark_read(app: &App<Box<dyn Provider>>, subject: &str, read: bool) {
    let snapshot = app.mailbox_list();
    let RowKey::Message { account, key } = row_by_subject(&snapshot, subject).key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::MarkRead {
        row: RowRef::Message(MessageRef { account, key }),
        read,
    })
    .await
    .expect("the mark-read dispatch");
}
