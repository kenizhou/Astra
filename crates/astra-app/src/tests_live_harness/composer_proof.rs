// SPDX-License-Identifier: GPL-3.0-only
//! The composer's finishing surfaces proven live over the harness wire: the
//! three delivery proofs the formatting, signature, and link contracts rest
//! on. Each rides the app's own submit, sync, and reading pipeline (the send
//! proof's both_accounts_app shape: astra1 sends, astra2 reads back), each
//! runs under a subject unique among the harness fixtures, and each withdraws
//! both copies so the harness keeps its seeded shape.
//!
//! The formatting proof pins the three widened marks surviving a real
//! delivery: a document carrying a font size run, a colour run, and a
//! highlight run sends over the dial, and the recipient's reading snapshot
//! carries the concrete rendered attributes (each span's inline CSS), not
//! merely the words. The signature proof pins the Signature block's two legs:
//! the plain part carries the RFC 3676 delimiter line and the signature's own
//! words, and the html part carries the signature div. The link proof pins
//! the scheme rule both ways: a good https target arrives as an anchor in the
//! received html, while a javascript: target in the sent document's wire
//! degrades to plain text before the send, so the received body carries the
//! words with no anchor and no trace of the scheme. The degradation is the
//! wire's own: the document is deserialised from the JSON an editor emits,
//! the refused target maps to no link, and the proof pins that mapping on
//! the parsed document before the send rides it.

use astra_composer::{
    Block, ComposerDocument, FontSize, InlineContent, Paragraph, Signature, TextColor, TextRun,
};
use astra_viewmodel::{ReadingSnapshot, RowKey};
use engine_api::{AccountId, Provider};

use crate::{reference::MessageRef, App, FolderRef, Intent};

use super::{harness, inbox_key, proof, row_by_subject};

/// The subject the formatting proof pins: unique among the harness fixtures,
/// so a row carrying it is this proof's mail and nothing the seed delivered.
const FORMATTING_SUBJECT: &str = "Harness composer formatting proof";

/// The words the font size run carries, the largest step the toolbar offers.
const SIZE_WORDS: &str = "Huge words ride the wire. ";

/// The words the colour run carries.
const COLOUR_WORDS: &str = "Rose words ride the wire. ";

/// The words the highlight run carries.
const HIGHLIGHT_WORDS: &str = "Amber words ride the wire.";

/// The palette value the colour run paints with (the toolbar's Rose).
const ROSE: &str = "#f43f5e";

/// The palette value the highlight run paints with (the toolbar's Amber).
const AMBER: &str = "#f59e0b";

/// The subject the signature proof pins: unique among the harness fixtures.
const SIGNATURE_SUBJECT: &str = "Harness composer signature proof";

/// The typed words the signature proof's document carries above its block.
const SIGNATURE_PROOF_WORDS: &str = "The signature proof's typed words.";

/// The signature's own words, both legs carry them.
const SIGNATURE_WORDS: &str = "The harness signature's own words.";

/// The subject the link proof pins: unique among the harness fixtures.
const LINK_SUBJECT: &str = "Harness composer link proof";

/// The words the good link's run carries.
const GOOD_LINK_WORDS: &str = "the schedule page";

/// The https target the good link points at.
const GOOD_LINK_TARGET: &str = "https://example.test/composer-proof";

/// The words the refused run carries, sent as plain text.
const REFUSED_WORDS: &str = "the refused target";

/// A formatting round trip over the live harness: a document carrying a font
/// size run, a colour run, and a highlight run sends from astra1 to astra2
/// over the real dial, the recipient's reading snapshot opens the body, and
/// the formatting survives as the concrete inline CSS each mark renders, with
/// the words riding the plain leg unmarked. Both copies are withdrawn.
#[tokio::test]
async fn a_formatted_send_round_trips_its_marks() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    withdraw(&app, &sender, &recipient, FORMATTING_SUBJECT).await;

    let reading = send_and_read_back(
        &app,
        &sender,
        &recipient,
        FORMATTING_SUBJECT,
        formatted_document(),
    )
    .await;

    let html = reading
        .html
        .as_deref()
        .expect("the formatted body's html leg");
    assert!(
        html.contains("font-size:24px"),
        "the size run's font-size attribute survives the delivery: {html}"
    );
    assert!(
        html.contains("color:#f43f5e"),
        "the colour run's color attribute survives the delivery: {html}"
    );
    assert!(
        html.contains("background-color:#f59e0b"),
        "the highlight run's background-color attribute survives the delivery: {html}"
    );
    assert!(
        html.contains(SIZE_WORDS) && html.contains(COLOUR_WORDS) && html.contains(HIGHLIGHT_WORDS),
        "every run's words ride the received html: {html}"
    );
    let folded = folding(reading.plain.as_deref());
    assert!(
        folded.contains(SIZE_WORDS.trim_end())
            && folded.contains(COLOUR_WORDS.trim_end())
            && folded.contains(HIGHLIGHT_WORDS),
        "every run's words ride the received plain leg, unmarked: {folded:?}"
    );
    assert!(
        !folded.contains("font-size") && !folded.contains("background-color"),
        "the plain leg carries no markup: {folded:?}"
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    withdraw(&app, &sender, &recipient, FORMATTING_SUBJECT).await;
    assert_withdrawn(&app, &recipient, FORMATTING_SUBJECT).await;
}

/// A signature-bearing round trip over the live harness: a document carrying
/// a Signature block sends, and the recipient's plain leg carries the RFC
/// 3676 delimiter on its own line beside the signature's words while the html
/// leg carries the signature div wrapping them. Both copies are withdrawn.
#[tokio::test]
async fn a_signature_bearing_send_round_trips_both_legs() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    withdraw(&app, &sender, &recipient, SIGNATURE_SUBJECT).await;

    let reading = send_and_read_back(
        &app,
        &sender,
        &recipient,
        SIGNATURE_SUBJECT,
        signature_document(),
    )
    .await;

    let folded = folding(reading.plain.as_deref());
    assert!(
        folded.contains("\n-- \n"),
        "the plain leg carries the RFC 3676 delimiter on its own line: {folded:?}"
    );
    assert!(
        folded.contains(SIGNATURE_WORDS),
        "the plain leg carries the signature's words: {folded:?}"
    );
    let html = reading
        .html
        .as_deref()
        .expect("the signature body's html leg");
    assert!(
        html.contains("<div class=\"allodia-signature\">"),
        "the html leg carries the signature div: {html}"
    );
    assert!(
        html.contains(SIGNATURE_WORDS),
        "the html leg carries the signature's words: {html}"
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    withdraw(&app, &sender, &recipient, SIGNATURE_SUBJECT).await;
    assert_withdrawn(&app, &recipient, SIGNATURE_SUBJECT).await;
}

/// A link round trip over the live harness, both ways: a good https target
/// arrives as an anchor in the received html (the only anchor the body
/// carries), and a javascript: target in the sent document's wire degrades to
/// plain text before the send, its words arriving bare with no anchor and no
/// trace of the scheme anywhere in either leg. Both copies are withdrawn.
#[tokio::test]
async fn a_link_round_trips_both_ways() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient) = proof::both_accounts_app(&host, &port, &user, &password).await;

    withdraw(&app, &sender, &recipient, LINK_SUBJECT).await;

    let reading =
        send_and_read_back(&app, &sender, &recipient, LINK_SUBJECT, link_document()).await;

    let html = reading.html.as_deref().expect("the linked body's html leg");
    // The reading sanitiser adds its own rel attribute beside the href, so the
    // anchor is pinned by its href and by the words it wraps; the one-anchor
    // count below pairs the two beyond doubt.
    assert!(
        html.contains(&format!("<a href=\"{GOOD_LINK_TARGET}\"")),
        "the good target arrives as an anchor: {html}"
    );
    assert!(
        html.contains(&format!(">{GOOD_LINK_WORDS}</a>")),
        "the anchor carries the good link's words: {html}"
    );
    assert_eq!(
        html.matches("<a href=").count(),
        1,
        "the good link is the body's only anchor, the refused target never forms one: {html}"
    );
    assert!(
        html.contains(REFUSED_WORDS),
        "the refused run's words ride the received html as plain text: {html}"
    );
    assert!(
        !html.to_ascii_lowercase().contains("javascript:"),
        "no trace of the refused scheme rides the received html: {html}"
    );
    let folded = folding(reading.plain.as_deref());
    assert!(
        folded.contains(&format!("{GOOD_LINK_WORDS} <{GOOD_LINK_TARGET}>")),
        "the plain leg carries the good link in its words-plus-target form: {folded:?}"
    );
    assert!(
        folded.contains(REFUSED_WORDS) && !folded.contains("javascript:"),
        "the plain leg carries the refused words with no scheme beside them: {folded:?}"
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    withdraw(&app, &sender, &recipient, LINK_SUBJECT).await;
    assert_withdrawn(&app, &recipient, LINK_SUBJECT).await;
}

/// The formatting proof's document: one paragraph, three runs, one widened
/// mark each (a font size, a text colour, a highlight), the palette values
/// the toolbar's pickers offer.
fn formatted_document() -> ComposerDocument {
    let run = |text: &str| TextRun {
        text: text.to_owned(),
        bold: false,
        italic: false,
        underline: false,
        font_size: None,
        color: None,
        highlight: None,
        link: None,
    };
    let mut huge = run(SIZE_WORDS);
    huge.font_size = Some(FontSize::Huge);
    let mut rose = run(COLOUR_WORDS);
    rose.color = TextColor::new(ROSE);
    let mut amber = run(HIGHLIGHT_WORDS);
    amber.highlight = TextColor::new(AMBER);
    ComposerDocument {
        blocks: vec![Block::Paragraph(Paragraph {
            content: vec![
                InlineContent::Text(huge),
                InlineContent::Text(rose),
                InlineContent::Text(amber),
            ],
        })],
        attachments: Vec::new(),
    }
}

/// The signature proof's document: the typed words above a Signature block,
/// the shape a composition seeded from the settings editor sends.
fn signature_document() -> ComposerDocument {
    ComposerDocument {
        blocks: vec![
            proof::paragraph(SIGNATURE_PROOF_WORDS),
            Block::Signature(Signature {
                body_html: format!("<p>{SIGNATURE_WORDS}</p>"),
                body_plain: SIGNATURE_WORDS.to_owned(),
            }),
        ],
        attachments: Vec::new(),
    }
}

/// The link proof's document, deserialised from the wire JSON an editor
/// emits, so the refused target rides the document exactly as a mistaken or
/// hostile editor would hand it over. The good target parses into a link and
/// the javascript: target maps to none, and the mapping is pinned here before
/// the send rides the document: the degradation is the core's own rule, not
/// the proof's.
fn link_document() -> ComposerDocument {
    let document: ComposerDocument = serde_json::from_value(serde_json::json!({
        "blocks": [{
            "Paragraph": { "content": [
                { "Text": { "text": "The schedule lives on " } },
                { "Text": { "text": GOOD_LINK_WORDS, "link": GOOD_LINK_TARGET } },
                { "Text": { "text": " and " } },
                { "Text": { "text": REFUSED_WORDS, "link": "javascript:alert(1)" } },
                { "Text": { "text": " degrades." } }
            ] }
        }],
        "attachments": []
    }))
    .expect("the wire document parses");
    let mut runs: Vec<&TextRun> = Vec::new();
    for block in &document.blocks {
        let Block::Paragraph(paragraph) = block else {
            panic!("the wire document holds one paragraph");
        };
        for inline in &paragraph.content {
            if let InlineContent::Text(run) = inline {
                runs.push(run);
            }
        }
    }
    assert!(
        runs.iter().any(|run| run.text == GOOD_LINK_WORDS
            && run
                .link
                .as_ref()
                .is_some_and(|link| link.as_str() == GOOD_LINK_TARGET)),
        "the good target parses into a link: {:?}",
        runs.iter().map(|run| &run.text).collect::<Vec<_>>()
    );
    assert!(
        runs.iter()
            .any(|run| run.text == REFUSED_WORDS && run.link.is_none()),
        "the refused target degrades to no link on the wire: {:?}",
        runs.iter().map(|run| &run.text).collect::<Vec<_>>()
    );
    document
}

/// Presents the store, withdraws every copy of `subject` from both accounts,
/// and lands the deletes: the converging sweep a proof opens and closes with,
/// so a run interrupted before its cleanup still converges and the harness
/// keeps its seeded shape.
async fn withdraw(
    app: &App<Box<dyn Provider>>,
    sender: &AccountId,
    recipient: &AccountId,
    subject: &str,
) {
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(app, sender, &[subject]).await;
    proof::delete_subject_mail(app, recipient, &[subject]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
}

/// Sends `document` under `subject` from `sender` to the recipient, syncs,
/// and opens the delivered copy through the product's own reading pipeline:
/// every proof's delivery half. The queue is asserted empty, the gesture's
/// own drain being what empties it.
async fn send_and_read_back(
    app: &App<Box<dyn Provider>>,
    sender: &AccountId,
    recipient: &AccountId,
    subject: &str,
    document: ComposerDocument,
) -> ReadingSnapshot {
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: subject.to_owned(),
        document,
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    })
    .await
    .unwrap();
    assert!(
        app.sending().rows.is_empty(),
        "a delivered send leaves nothing in the queue"
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(app, recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, subject);
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::OpenMessage {
        message: MessageRef { account, key },
    })
    .await
    .unwrap();
    app.reading()
        .expect("the open publishes a reading snapshot")
}

/// Asserts the recipient's INBOX no longer lists `subject`: the withdrawal
/// held, and a rerun starts from the seeded shape.
async fn assert_withdrawn(app: &App<Box<dyn Provider>>, recipient: &AccountId, subject: &str) {
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(app, recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    assert!(
        snapshot.rows.iter().all(|row| row.subject != subject),
        "the delivered copy is withdrawn from the recipient's INBOX"
    );
}

/// The plain leg with its transport line endings folded to newlines, so a
/// delimiter assertion reads the body's lines and not its wire encoding.
fn folding(plain: Option<&str>) -> String {
    plain.unwrap_or_default().replace("\r\n", "\n")
}
