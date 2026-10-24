// SPDX-License-Identifier: GPL-3.0-only
//! The gated reading proofs against the live harness: the reading batch's
//! three live contracts, each over the same accepted-certificate dial the
//! other gated suites use and each withdrawing its own mail through the
//! product's delete so the harness keeps its seeded shape.
//!
//! The `.eml` proof pins the export's byte honesty. Its message is submitted
//! through the harness's own SMTP submission listener (curl over implicit
//! TLS from the host, the same listener the seed rides inside the container)
//! because the app's own serialiser writes canonical shapes only: unfolded
//! headers and unencoded utf-8 bodies, so a composer send cannot carry the
//! folded headers and the quoted-printable body whose preservation is what
//! byte honesty means. Every read is the app's: the delivery is observed
//! through the product's sync, the words open through the reading pipeline
//! (the quoted-printable decoding is the pipeline's own), the export read
//! through `eml_content` matches a fresh engine `message_source` read byte
//! for byte, and the received source carries the non-canonical shapes: the
//! folded subject, the folded trace header, the soft line break, the escapes,
//! and the `Delivered-To` header the server prepends.
//!
//! The reflow proof pins the clamp's subject. A body whose table carries a
//! hard pixel width is delivered through the same submission wire and opens
//! through the reading pipeline, and the sanitised html the wrapper clamps
//! keeps the width in both shapes a sender writes it (the attribute and the
//! inline rule). The wrapper's clamp itself is CSS inside `wrapBodyDocument`
//! (client/web/src/components/reading-pane/MessageHtml.tsx), pinned by that
//! file's offline tests; neither harness here lays out a document, so no
//! instrument renders one body at two frame widths, and the clamp being one
//! width-relative rule (`max-width:100%`, reading whatever the width is) is
//! exactly why the wrapper carries no width parameter to vary.
//!
//! The junk proof pins the report's filing. A note the run submits through
//! the app's own send pipeline is reported through the app's `mark_junk` over
//! the real dial, the filing observed through the app's own folder view (the
//! pane's list after a refresh, the sync's word for the server: the row
//! stands in the account's Junk folder and is gone from the INBOX); the
//! `mark_not_junk` correction, dispatched over the Junk folder's own row,
//! files it back to the INBOX the same way. The `$junk` keyword half of the
//! report is not observable through the IMAP verbs the app holds; the filing
//! is the observable contract, and this proof words itself by it. The run
//! withdraws every copy it made (the delivered note and the sender's filed
//! copy) through the product's own delete.

use astra_viewmodel::{FolderRole, RowKey};

use crate::{reference::MessageRef, reference::RowRef, FolderRef, Intent};

use super::{
    harness, inbox_key,
    proof::{self, document},
    row_by_subject,
};

/// The subject the `.eml` proof pins: the fold's unfolded value, which is
/// what the engine's envelope reads a properly folded subject as.
const EML_SUBJECT: &str = "Harness eml proof with a folded header line";

/// The raw message the `.eml` proof submits: the folded subject and the
/// folded `X-Astra-Proof` header, a quoted-printable body carrying an `=`
/// escape, a non-ascii word, and a soft line break. CRLF throughout, and the
/// continuation lines' leading spaces are content, so the literal is a
/// `concat!` whose pieces keep their own leading whitespace (a backslash
/// line skip in a plain literal would eat exactly those spaces).
const EML_RAW: &str = concat!(
    "From: Astra One <astra1@example.test>\r\n",
    "To: <astra2@example.test>\r\n",
    "Subject: Harness eml proof with a folded\r\n",
    " header line\r\n",
    "X-Astra-Proof: one\r\n",
    " two\r\n",
    "Date: Sat, 24 Oct 2026 10:00:00 +0000\r\n",
    "Message-ID: <harness-eml-proof@example.test>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: text/plain; charset=utf-8\r\n",
    "Content-Transfer-Encoding: quoted-printable\r\n",
    "\r\n",
    "The eml proof body with an =3D escape and a non-ascii caf=C3=A9 and a soft=\r\n",
    " break.\r\n",
);

/// The words the `.eml` proof's body decodes to, what the reading pipeline
/// opens: the escape unescaped, the non-ascii word rebuilt, the soft break
/// joined, and the body's trailing CRLF kept.
const EML_DECODED: &str =
    "The eml proof body with an = escape and a non-ascii café and a soft break.\r\n";

/// The subject the reflow proof pins.
const REFLOW_SUBJECT: &str = "Harness reflow proof";

/// The raw message the reflow proof submits: an html body whose table
/// carries a hard pixel width in both shapes a sender writes it, the
/// presentational attribute and the inline rule. CRLF throughout, built as a
/// `concat!` so every line keeps the bytes it names.
const REFLOW_RAW: &str = concat!(
    "From: Astra One <astra1@example.test>\r\n",
    "To: <astra2@example.test>\r\n",
    "Subject: Harness reflow proof\r\n",
    "Date: Sat, 24 Oct 2026 10:05:00 +0000\r\n",
    "Message-ID: <harness-reflow-proof@example.test>\r\n",
    "MIME-Version: 1.0\r\n",
    "Content-Type: text/html; charset=utf-8\r\n",
    "\r\n",
    "<p>The reflow proof body before the table.</p>\r\n",
    "<table width=\"760\" style=\"width:760px\"><tr><td>The fixed-width cell.</td></tr></table>\r\n",
);

/// The subject the junk proof pins.
const JUNK_SUBJECT: &str = "Harness junk proof";

/// The words the junk proof's note carries.
const JUNK_WORDS: &str = "The junk proof note, reported and corrected over the real dial.";

/// Submits `message`'s raw bytes from astra1 to astra2 through the harness's
/// SMTP submission listener: curl over implicit TLS from the host, the same
/// listener the seed rides inside the container and the app's own send rides
/// from the sender's side. Seeding only, never an assertion path: the
/// assertion half is every read that follows, all through the product.
fn submit_raw(host: &str, password: &str, message: &str) {
    let path = std::env::temp_dir().join(format!("astra-reading-proof-{}.eml", std::process::id()));
    std::fs::write(&path, message.as_bytes()).expect("the raw submission file writes");
    let output = std::process::Command::new("curl")
        .arg("-sk")
        .arg("--user")
        .arg(format!("astra1@example.test:{password}"))
        .arg("--mail-from")
        .arg("astra1@example.test")
        .arg("--mail-rcpt")
        .arg(proof::RECIPIENT)
        .arg("--url")
        .arg(format!("smtps://{host}:{}", super::harness_smtp_port()))
        .arg("-T")
        .arg(&path)
        .output()
        .expect("the raw submission spawns");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "the raw submission delivered: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The `.eml` export against the live harness: a message carrying folded
/// headers and a quoted-printable body is delivered through the submission
/// wire, opens through the reading pipeline with the body decoded, and the
/// app-level export read matches a fresh engine source read byte for byte
/// while the received source keeps every non-canonical shape, and the run
/// leaves both accounts as it found them.
#[tokio::test]
async fn an_eml_export_matches_the_received_source_byte_for_byte() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    // The converging sweep every delivery proof opens with.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[EML_SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[EML_SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    submit_raw(&host, &password, EML_RAW);

    // The delivery, observed through the product's own sync and list.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, EML_SUBJECT);
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };

    // The open: the pipeline decodes the quoted-printable body, so the words
    // read back the decoded sentence, never the escapes. The subject the
    // snapshot carries is the fold's unfolded value; the bytes below keep
    // the fold whole.
    app.dispatch(Intent::OpenMessage {
        message: MessageRef {
            account: account.clone(),
            key: key.clone(),
        },
    })
    .await
    .unwrap();
    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert_eq!(reading.subject, EML_SUBJECT);
    assert_eq!(reading.plain.as_deref(), Some(EML_DECODED));

    // A fresh engine source read, then the app-level export over the same
    // message: the export's bytes are the source's bytes, byte for byte.
    let handle = app
        .account_handle(&account)
        .await
        .expect("the recipient's account");
    let provider = handle.providers.first().expect("the recipient's provider");
    let original = crate::reading::resolve(
        &app.engine,
        &MessageRef {
            account: account.clone(),
            key: key.clone(),
        },
    )
    .await
    .expect("the message resolves in the synced set");
    let source = app
        .engine
        .message_source(provider, &account, &original)
        .await
        .expect("the engine source read")
        .into_bytes();
    let exported = app
        .eml_content(&account, &key)
        .await
        .expect("the app-level export read");
    assert_eq!(
        exported.bytes, source,
        "the export's bytes are the engine's source bytes, never a re-serialisation"
    );

    // The suggested name, derived from the unfolded subject.
    assert_eq!(
        exported.file_name,
        format!("{EML_SUBJECT}.eml"),
        "the export suggests the sanitised subject"
    );

    // The received source carries the non-canonical shapes: the folds, the
    // quoted-printable markers, the soft break, and the server's own
    // prepended delivery header, none of which any serialiser of ours writes.
    let text = String::from_utf8_lossy(&exported.bytes);
    assert!(
        text.contains("Subject: Harness eml proof with a folded\r\n header line"),
        "the subject keeps its fold, source: {text}"
    );
    assert!(
        text.contains("X-Astra-Proof: one\r\n two"),
        "the trace header keeps its fold, source: {text}"
    );
    assert!(
        text.contains("Content-Transfer-Encoding: quoted-printable"),
        "the body keeps its transfer encoding, source: {text}"
    );
    assert!(
        text.contains("=3D") && text.contains("caf=C3=A9"),
        "the escapes stay escaped, source: {text}"
    );
    assert!(
        text.contains("soft=\r\n break."),
        "the soft line break stays folded, source: {text}"
    );
    assert!(
        text.contains("Delivered-To: astra2@example.test"),
        "the server's own delivery header rides the source, source: {text}"
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    // The cleanup: both accounts swept, the harness keeping its shape.
    proof::delete_subject_mail(&app, &sender, &[EML_SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[EML_SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot.rows.iter().all(|row| row.subject != EML_SUBJECT),
        "the delivered copy is withdrawn from the recipient's INBOX"
    );
}

/// The reflow proof against the live harness: a fixed-width table body
/// delivered through the submission wire keeps its hard pixel width through
/// the pipeline into the sanitised html the wrapper clamps, in both shapes
/// the sender wrote it, and the run leaves both accounts as it found them.
#[tokio::test]
async fn a_fixed_width_body_keeps_its_hard_width_into_the_sanitised_html() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[REFLOW_SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[REFLOW_SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    submit_raw(&host, &password, REFLOW_RAW);

    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, REFLOW_SUBJECT);
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };

    // The open: the sanitised html keeps the hard pixel width in both shapes
    // (the attribute and the inline rule), so the wrapper's clamp holds live
    // content, not a normalised body.
    app.dispatch(Intent::OpenMessage {
        message: MessageRef { account, key },
    })
    .await
    .unwrap();
    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    let html = reading.html.as_deref().expect("the sanitised html body");
    assert!(
        html.contains("The reflow proof body before the table."),
        "the body text survives sanitisation, html: {html}"
    );
    assert!(
        html.contains("width=\"760\"") && html.contains("width:760px"),
        "the hard pixel width survives in both its shapes, html: {html}"
    );
    assert!(
        html.contains("<td>The fixed-width cell.</td>"),
        "the table's content survives sanitisation, html: {html}"
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    proof::delete_subject_mail(&app, &sender, &[REFLOW_SUBJECT]).await;
    proof::delete_subject_mail(&app, &recipient, &[REFLOW_SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot
            .rows
            .iter()
            .all(|row| row.subject != REFLOW_SUBJECT),
        "the delivered copy is withdrawn from the recipient's INBOX"
    );
}

/// The junk report against the live harness: a note the run submits through
/// the app's own send pipeline is filed into Junk by the app's `mark_junk`
/// and filed back to the INBOX by its `mark_not_junk` correction, both
/// filings observed through the app's own folder views, and the run
/// withdraws every copy it made.
#[tokio::test]
async fn a_junk_report_files_the_note_and_the_correction_files_it_back() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, _recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    // The converging sweep, over both halves of the run's own mail.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[JUNK_SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The run's own note, submitted through the app's send pipeline and
    // delivered to the sender's own INBOX by the harness.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: "astra1@example.test".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: JUNK_SUBJECT.to_owned(),
        document: document(JUNK_WORDS),
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    })
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let inbox = FolderRef::from_parts(sender.as_str(), inbox_key(&app, &sender))
        .expect("the sender's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, JUNK_SUBJECT);
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };

    // The report: one act through the app's own verb, over the real dial.
    app.mark_junk(RowRef::Message(MessageRef { account, key }))
        .await
        .expect("the junk report");
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The filing, observed through the app's own folder view: the note stands
    // in the account's Junk folder and is gone from the INBOX.
    let junk = FolderRef::from_parts(
        sender.as_str(),
        proof::role_key(&app, &sender, FolderRole::Junk),
    )
    .expect("the account's Junk folder reference");
    app.dispatch(Intent::SelectFolder { folder: junk })
        .await
        .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let snapshot = app.mailbox_list();
    let filed = row_by_subject(&snapshot, JUNK_SUBJECT);
    let RowKey::Message { account, key } = filed.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    let inbox = FolderRef::from_parts(sender.as_str(), inbox_key(&app, &sender))
        .expect("the sender's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot.rows.iter().all(|row| row.subject != JUNK_SUBJECT),
        "the reported note is gone from the INBOX"
    );

    // The correction, dispatched over the Junk folder's own row (the row's
    // key is the message's key in the folder it now sits in), files it back.
    app.mark_not_junk(RowRef::Message(MessageRef { account, key }))
        .await
        .expect("the not-junk correction");
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(sender.as_str(), inbox_key(&app, &sender))
        .expect("the sender's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        row_by_subject(&snapshot, JUNK_SUBJECT).subject == JUNK_SUBJECT,
        "the corrected note is back in the INBOX"
    );
    let junk = FolderRef::from_parts(
        sender.as_str(),
        proof::role_key(&app, &sender, FolderRole::Junk),
    )
    .expect("the account's Junk folder reference");
    app.dispatch(Intent::SelectFolder { folder: junk })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot.rows.iter().all(|row| row.subject != JUNK_SUBJECT),
        "the corrected note is gone from the Junk folder"
    );

    // The cleanup: every copy the run made (the delivered note, the sender's
    // filed copy) withdrawn through the product's own delete.
    proof::delete_subject_mail(&app, &sender, &[JUNK_SUBJECT]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(sender.as_str(), inbox_key(&app, &sender))
        .expect("the sender's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot.rows.iter().all(|row| row.subject != JUNK_SUBJECT),
        "the run's note is withdrawn from the INBOX"
    );
}
