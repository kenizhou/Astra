// SPDX-License-Identifier: GPL-3.0-only
//! The gated JMAP proofs against the harness's plaintext JMAP origin: the
//! password account end to end, the calendar read the session itself
//! advertises, and the OAuth gate's honest refusal.
//!
//! Every leg rides the product's own paths: the dial through
//! [`dial_jmap_account`](astra_account::dial_jmap_account) (the one
//! account-wide provider), the lists and bodies through the engine's sync
//! and reading pipeline, the send through the engine's submission face,
//! and the calendar through the engine's JMAP calendars sync. The one
//! seeding helper rides CalDAV raw (the `calendar_proof` fixtures): the
//! harness keeps one calendar store beneath both faces, so an event
//! seeded over CalDAV and read back over JMAP proves the JMAP calendar
//! leg against the server's own state, not a fixture's echo.
//!
//! The OAuth proof is the honest half the harness can give: the origin
//! serves the RFC 9728 protected-resource document (its issuer is
//! `https://mail.example.test`, a name that resolves nowhere, the finding
//! the SP2l discovery proof recorded), so the 8414 hop cannot complete
//! and [`jmap_signin_offered`](astra_account::jmap_signin_offered) must
//! answer false: no sign-in is offered and the password field stands.
//!
//! The send withdraws both copies through the engine's own delete, so the
//! harness keeps its seeded shape; the opened rich note is put back to
//! unread the same way, so the IMAP suites' known-state resets converge.

use std::sync::Arc;

use astra_viewmodel::{FolderRole, RowKey};
use engine_api::{AccountId, EmailAddress, Engine, Provider};
use engine_tls::TlsPolicy;

use astra_account::{
    dial_jmap_account, jmap_signin_offered, AccountConfig, AccountShape, JmapAccount, Secret,
};

use crate::{reference::MessageRef, Account, App, AppObserver, CalendarIntent, FolderRef, Intent};

use super::calendar_proof::{dav, event_ics, range, rows_of, run_token};
use super::{harness, inbox_key, jmap_base, proof, row_by_subject, CountingObserver};

/// The subject the send proof pins: unique among the harness fixtures, so a
/// row carrying it is this proof's mail and nothing the seed delivered.
const SUBJECT: &str = "Harness jmap send proof";

/// The words the composer's document carries; what the reading pane opens back.
const WORDS: &str = "Sent from the composer over the harness jmap submission.";

/// The stored-config document of one JMAP harness account: the `[jmap]`
/// section alone, exactly the table the setup form's builder writes.
fn jmap_config(base: &str, user: &str, password: &str) -> AccountConfig {
    AccountConfig {
        imap: None,
        smtp: None,
        certificate_exceptions: Vec::new(),
        graph: None,
        google: None,
        jmap: Some(JmapAccount {
            email: user.to_owned(),
            base_url: base.to_owned(),
            password: Some(Secret::new(password.to_owned())),
            oauth: None,
        }),
        dav: None,
        shape: AccountShape::default(),
    }
}

/// Dials one JMAP harness account through the product's own dial and returns
/// its derived account id with the one account-wide provider.
async fn dial_jmap(base: &str, user: &str, password: &str) -> (AccountId, Box<dyn Provider>) {
    let config = jmap_config(base, user, password);
    let account_id = config.account_id().expect("the jmap account id derives");
    let tls = engine_tls::client_config(&TlsPolicy::bundled_and_system())
        .expect("the account tls policy builds");
    let provider = dial_jmap_account(&config, &tls)
        .await
        .expect("the jmap dial over the harness origin");
    (account_id, Box::new(provider) as Box<dyn Provider>)
}

/// The account's folder key carrying `name`, as the pane projects it (Projects
/// carries no standard role, so the name is the only handle).
fn folder_named(app: &App<Box<dyn Provider>>, account: &AccountId, name: &str) -> String {
    app.mailbox_list()
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .and_then(|row| row.folders.iter().find(|folder| folder.name == name))
        .map_or_else(
            || panic!("the pane lists the account's {name} folder"),
            |folder| folder.key.as_str().to_owned(),
        )
}

/// The password account end to end over the harness's JMAP origin: the dial
/// snapshots the session's own capabilities (mail, submission, calendars,
/// read from the server, never guessed), one refresh lists the account's
/// folders through the account-wide provider, the seeded notes render in the
/// INBOX newest first, the rich note opens through the reading pipeline
/// (the remote image gated, the inline one resolved), a send round-trips to
/// the second JMAP account with the sender's copy filed in Sent, and the run
/// withdraws everything it made.
#[tokio::test]
async fn the_password_account_syncs_reads_and_sends_end_to_end_over_jmap() {
    let Some((_host, _port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let base = jmap_base().expect("the gated run names the harness origin");

    let (sender, sender_provider) = dial_jmap(&base, &user, &password).await;
    let (recipient, recipient_provider) = dial_jmap(&base, proof::RECIPIENT, &password).await;

    // The session's own advertisement, the dial's snapshot discipline: the
    // harness session names mail, submission, and calendars, and nothing
    // here guesses from the config.
    let capabilities = sender_provider.connection_info().capabilities;
    assert!(capabilities.mail(), "the session advertises mail");
    assert!(
        capabilities.submission(),
        "the session advertises submission"
    );
    assert!(
        capabilities.calendars(),
        "the session advertises calendars, the leg the calendar proof rides"
    );

    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![
            Account {
                id: sender.clone(),
                providers: vec![sender_provider],
                identity: EmailAddress::new(user.as_str()),
            },
            Account {
                id: recipient.clone(),
                providers: vec![recipient_provider],
                identity: EmailAddress::new(proof::RECIPIENT),
            },
        ],
        None,
        observer,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The folder list through the one account-wide provider: the reserved
    // INBOX under its role, the seed's custom folder under its name.
    let inbox = FolderRef::from_parts(sender.as_str(), inbox_key(&app, &sender))
        .expect("the INBOX folder reference");
    let projects = FolderRef::from_parts(sender.as_str(), folder_named(&app, &sender, "Projects"))
        .expect("the Projects folder reference");
    assert_ne!(inbox.key, projects.key, "two folders, two keys");

    // The three seeded notes, newest first: the same notes the IMAP proofs
    // pin, read back over the JMAP sync.
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let subjects: Vec<&str> = snapshot
        .rows
        .iter()
        .map(|row| row.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        ["Harness note three", "Harness note two", "Harness note one"],
        "the whole INBOX is the three seeded notes, newest first",
    );
    let one = row_by_subject(&snapshot, "Harness note one");
    assert_eq!(one.sender_name, "Astra Two");
    assert_eq!(one.sender_address, "astra2@example.test");

    // The body read: the rich note opens through the product's own reading
    // pipeline, the remote image gated behind the placeholder and the inline
    // one resolved to a data URI, whatever transport carried the bytes.
    app.dispatch(Intent::SelectFolder { folder: projects })
        .await
        .unwrap();
    let key = row_by_subject(&app.mailbox_list(), "Harness rich note")
        .key
        .clone();
    let RowKey::Message { account, key } = key else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::OpenMessage {
        message: MessageRef { account, key },
    })
    .await
    .unwrap();
    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert_eq!(reading.subject, "Harness rich note");
    assert_eq!(reading.from_name, "Astra Two");
    let html = reading.html.as_deref().expect("the sanitised html body");
    assert!(
        html.contains(r#"data-astra-src="https://astra-invalid.example/px.png""#),
        "the remote reference is gated behind the placeholder, html: {html}"
    );
    assert!(
        html.contains("data:image/png;base64,"),
        "the inline cid: image arrived as a data URI, html: {html}"
    );
    app.dispatch(Intent::CloseReading).await.unwrap();
    super::mark_read(&app, "Harness rich note", false).await;

    // Back to a known state before the send: a run interrupted before its
    // cleanup may have left the proof mail behind, and the sweep converges.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The send: the composer's gesture, submitted through the session's
    // submission capability and drained inside the same dispatch.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: proof::document(WORDS),
        blobs: Vec::new(),
    })
    .await
    .unwrap();
    assert!(
        app.sending().rows.is_empty(),
        "a delivered send leaves nothing in the queue"
    );

    // The delivery: after one refresh the recipient's INBOX holds the mail
    // and the words open in the reading pane.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let recipient_inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder {
        folder: recipient_inbox,
    })
    .await
    .unwrap();
    let row = row_by_subject(&app.mailbox_list(), SUBJECT).key.clone();
    let RowKey::Message { account, key } = row else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::OpenMessage {
        message: MessageRef { account, key },
    })
    .await
    .unwrap();
    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert_eq!(reading.subject, SUBJECT);
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "the delivered body is the composer's words: {:?}",
        reading.plain
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    // The sender's half: the copy the submission filed in the Sent folder.
    let sent = FolderRef::from_parts(
        sender.as_str(),
        proof::role_key(&app, &sender, FolderRole::Sent),
    )
    .expect("the sender's Sent folder reference");
    app.dispatch(Intent::SelectFolder { folder: sent })
        .await
        .unwrap();
    let sent_snapshot = app.mailbox_list();
    let filed = row_by_subject(&sent_snapshot, SUBJECT);
    assert_eq!(
        filed.sender_address, "astra1@example.test",
        "the filed copy is the sender's own mail"
    );

    // The cleanup: both copies withdrawn, so a rerun starts clean and the
    // harness keeps its seeded shape.
    proof::delete_subject_mail(&app, &sender, &[SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    for account in [&sender, &recipient] {
        let snapshot = app.mailbox_list();
        assert!(
            snapshot.rows.iter().all(|row| row.subject != SUBJECT),
            "the proof mail is withdrawn from {account}'s selected folder"
        );
    }
}

/// The calendar read over the JMAP dial: an event seeded onto the account's
/// calendar through CalDAV (the raw seeding helper, one store beneath both
/// faces) renders in the calendar snapshot after one refresh, under the
/// calendars capability the dial's own session snapshot advertised. The
/// withdrawn seed leaves the window again.
#[tokio::test]
async fn the_calendars_the_session_advertised_read_over_the_jmap_dial() {
    let Some((_host, _port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let uid = format!("jmap-{token}@astra.local");
    let title = format!("Harness jmap calendar {token}");
    assert_eq!(
        dav(
            "PUT",
            &user,
            &uid,
            &password,
            &event_ics(
                &uid,
                &title,
                "DTSTART:20261103T100000Z",
                "DTEND:20261103T110000Z",
                &[],
            ),
        ),
        201,
        "the calendar seed lands over caldav"
    );

    let base = jmap_base().expect("the gated run names the harness origin");
    let (account_id, provider) = dial_jmap(&base, &user, &password).await;
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account_id.clone(),
            providers: vec![provider],
            identity: EmailAddress::new(user.as_str()),
        }],
        None,
        observer,
    );
    app.dispatch(range()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    let snapshot = app.calendar();
    let rows = rows_of(&snapshot, &token);
    assert_eq!(
        rows.len(),
        1,
        "the seeded event reads over the jmap calendars sync"
    );
    assert_eq!(
        rows[0].account, account_id,
        "the occurrence renders on the jmap account"
    );
    assert_eq!(rows[0].title, title);

    assert_eq!(dav("DELETE", &user, &uid, &password, ""), 204);
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert!(
        rows_of(&app.calendar(), &token).is_empty(),
        "the withdrawn seed leaves the window"
    );
}

/// The OAuth gate's honest refusal, live: the harness origin serves the
/// RFC 9728 protected-resource document (its authorization_servers names
/// `https://mail.example.test`), but that issuer resolves nowhere, so the
/// RFC 8414 hop cannot complete and the offered gate answers false: no
/// browser sign-in is offered for this server and the password field
/// stands, the fail-soft rule the wizard's advertise arm rides.
#[tokio::test]
async fn the_oauth_gate_fails_soft_when_the_named_issuer_resolves_nowhere() {
    let Some(base) = jmap_base() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let offered = jmap_signin_offered("astra1@example.test", Some(base.as_str())).await;
    assert!(
        !offered,
        "the harness serves the 9728 document but its issuer resolves nowhere, \
         so no sign-in may be offered"
    );
}
