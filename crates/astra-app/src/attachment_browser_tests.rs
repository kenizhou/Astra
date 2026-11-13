// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the attachment browser aggregation: the flattened rows beside
//! their source metadata, the newest-first order, the per-account scan cap,
//! the coverage report's honest counts, the per-account error entry, and the
//! sender and mime filters. The fixtures live in `dispatch_fakes`, the
//! crate-root test module every dispatch-shaped test shares.

use std::sync::{Arc, Mutex};

use astra_viewmodel::{AttachmentBrowserFilter, MessageKey, SyncBodyCap};
use engine_api::{AccountId, EmailAddress, ProviderKey};

use crate::{dispatch_fakes as fakes, Intent, SettingsIntent, Surface};

/// One message carrying two attachments: a pdf (part 2, base64 `UERG`, the
/// three bytes `PDF`) and a png (part 3, base64 `iVBORw0KGgo=`, the eight
/// bytes of the PNG signature).
fn mixed_source() -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(
        b"Content-Type: multipart/mixed; boundary=\"mix\"\r\n\r\n\
          --mix\r\nContent-Type: text/plain; charset=\"utf-8\"\r\n\r\n\
          The launch is ready.\r\n",
    );
    raw.extend_from_slice(
        b"--mix\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\n\
          Content-Disposition: attachment; filename=\"report.pdf\"\r\n\
          Content-Transfer-Encoding: base64\r\n\r\nUERG\r\n",
    );
    raw.extend_from_slice(
        b"--mix\r\nContent-Type: image/png; name=\"logo.png\"\r\n\
          Content-Disposition: attachment; filename=\"logo.png\"\r\n\
          Content-Transfer-Encoding: base64\r\n\r\niVBORw0KGgo=\r\n",
    );
    raw.extend_from_slice(b"--mix--\r\n");
    raw
}

/// One message carrying a single pdf attachment, the same part shape the
/// reading pane's save tests seed.
fn pdf_source() -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(
        b"Content-Type: multipart/mixed; boundary=\"mix\"\r\n\r\n\
          --mix\r\nContent-Type: text/plain; charset=\"utf-8\"\r\n\r\n\
          The review is ready.\r\n",
    );
    raw.extend_from_slice(
        b"--mix\r\nContent-Type: application/pdf; name=\"notes.pdf\"\r\n\
          Content-Disposition: attachment; filename=\"notes.pdf\"\r\n\
          Content-Transfer-Encoding: base64\r\n\r\nUERG\r\n",
    );
    raw.extend_from_slice(b"--mix--\r\n");
    raw
}

/// Marks a message as carrying an attachment: the provider's guess the
/// search index files, the flag the browse's query matches.
fn attached(mut message: engine_api::Message) -> engine_api::Message {
    message.has_attachment = true;
    message
}

/// Re-addresses a message from a second sender, so the sender filter has two
/// distinct addresses to choose between.
fn from_grace(mut message: engine_api::Message) -> engine_api::Message {
    message.envelope.from = vec![EmailAddress::named("Grace Hopper", "grace@example.test")];
    message
}

/// An app over one account whose Inbox holds `m1` (Ada, two parts, 08:00)
/// and `m2` (Grace, one pdf, 09:30), both flagged as attached and sourced,
/// synced so the browse has a store and sources to read.
async fn browsed_mailbox(surfaces: &Arc<Mutex<Vec<Surface>>>) -> crate::App<fakes::FakeProvider> {
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![
            attached(fakes::delivered_at(
                fakes::mail("m1", "inbox", "Launch"),
                8,
                0,
            )),
            attached(from_grace(fakes::delivered_at(
                fakes::mail("m2", "inbox", "Review"),
                9,
                30,
            ))),
        ],
        vec![
            ("m1".to_owned(), mixed_source()),
            ("m2".to_owned(), pdf_source()),
        ],
    );
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// The filter a browse runs under: no sender, no mime prefix.
fn unfiltered() -> AttachmentBrowserFilter {
    AttachmentBrowserFilter::default()
}

#[tokio::test]
async fn the_browse_flattens_the_parts_beside_their_source_metadata() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = browsed_mailbox(&surfaces).await;

    let view = app.attachment_browser(unfiltered()).await;

    // m2 is the newer message, so its one part leads; m1's two parts follow
    // in the order the message carries them.
    let names: Vec<&str> = view.entries.iter().map(|e| e.file_name.as_str()).collect();
    assert_eq!(names, ["notes.pdf", "report.pdf", "logo.png"]);

    let report = &view.entries[1];
    assert_eq!(report.id, 2, "the pdf's message-scoped part id");
    assert_eq!(report.media_type, "application/pdf");
    assert_eq!(report.size, 3, "the decoded byte length");
    assert_eq!(
        report.source,
        MessageKey {
            account: AccountId::try_from("work").unwrap(),
            key: ProviderKey::new("m1").unwrap(),
        },
        "the row names the message a save or an open addresses",
    );
    assert_eq!(report.subject, "Launch");
    assert_eq!(report.from_name, "Ada Lovelace");
    assert_eq!(report.from_address, "ada@example.test");
    assert_eq!(report.sent_at, "2026-10-09T08:00:00Z");

    let logo = &view.entries[2];
    assert_eq!(logo.id, 3);
    assert_eq!(logo.media_type, "image/png");
    assert_eq!(logo.size, 8);
}

#[tokio::test]
async fn the_browse_orders_the_rows_newest_source_first() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = browsed_mailbox(&surfaces).await;

    let view = app.attachment_browser(unfiltered()).await;

    let keys: Vec<&str> = view.entries.iter().map(|e| e.source.key.as_str()).collect();
    assert_eq!(keys, ["m2", "m1", "m1"], "the newer message's parts lead");
}

#[tokio::test]
async fn the_browse_orders_newest_first_across_accounts() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // The older message lives in the first account, the newer in the second:
    // the answer interleaves by date, never by account block.
    let work = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![attached(fakes::delivered_at(
            fakes::mail("m1", "inbox", "Launch"),
            8,
            0,
        ))],
        vec![("m1".to_owned(), mixed_source())],
    );
    let personal = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![attached(fakes::delivered_at(
            fakes::mail("m2", "inbox", "Review"),
            9,
            30,
        ))],
        vec![("m2".to_owned(), pdf_source())],
    );
    let app = fakes::app(
        vec![
            fakes::account("work", work),
            fakes::account("personal", personal),
        ],
        &surfaces,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let view = app.attachment_browser(unfiltered()).await;

    let keys: Vec<&str> = view.entries.iter().map(|e| e.source.key.as_str()).collect();
    assert_eq!(keys, ["m2", "m1", "m1"], "the newer account's part leads");
}

#[tokio::test]
async fn the_coverage_report_counts_the_scan_honestly() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = browsed_mailbox(&surfaces).await;

    let view = app.attachment_browser(unfiltered()).await;

    assert_eq!(view.coverage.scanned, 2, "both flagged messages resolved");
    assert_eq!(view.coverage.indexed, 2, "both sources read");
    assert_eq!(view.coverage.skipped, 0);
    assert_eq!(view.coverage.failed_accounts, []);
    assert_eq!(view.coverage.scan_limit, 200, "the window is stated");
}

#[tokio::test]
async fn a_message_whose_source_refuses_is_a_skip_not_a_failure() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // m1 carries a source; m2 is flagged as attached but holds none, so its
    // fetch refuses and the browse skips it best-effort.
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![
            attached(fakes::delivered_at(
                fakes::mail("m1", "inbox", "Launch"),
                8,
                0,
            )),
            attached(fakes::delivered_at(
                fakes::mail("m2", "inbox", "Review"),
                9,
                30,
            )),
        ],
        vec![("m1".to_owned(), mixed_source())],
    );
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let view = app.attachment_browser(unfiltered()).await;

    assert_eq!(view.entries.len(), 2, "the readable message's parts stand");
    assert_eq!(view.coverage.scanned, 2);
    assert_eq!(view.coverage.indexed, 1);
    assert_eq!(view.coverage.skipped, 1, "the refused source is a skip");
    assert_eq!(view.coverage.failed_accounts, []);
}

#[tokio::test]
async fn an_account_with_no_provider_is_an_honest_error_entry() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![attached(fakes::mail("m1", "inbox", "Launch"))],
        vec![("m1".to_owned(), mixed_source())],
    );
    // The second account holds no provider: its scan cannot run, and the
    // coverage report names it rather than hiding the gap.
    let empty = crate::Account {
        id: AccountId::try_from("empty").unwrap(),
        providers: Vec::new(),
        identity: EmailAddress::new("me@empty.local"),
    };
    let app = fakes::app(vec![fakes::account("work", provider), empty], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let view = app.attachment_browser(unfiltered()).await;

    assert_eq!(view.entries.len(), 2, "the readable account's parts stand");
    assert_eq!(
        view.coverage.failed_accounts,
        vec![AccountId::try_from("empty").unwrap()],
        "the account whose scan could not run is named",
    );
}

#[tokio::test]
async fn the_scan_caps_at_the_window_per_account() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // 201 flagged messages, none sourced: the query returns at most the cap,
    // and every returned message is a skip, so the count pins the window.
    let messages: Vec<_> = (0..201)
        .map(|i| {
            attached(fakes::delivered_at(
                fakes::mail(&format!("m{i:03}"), "inbox", "Bulk"),
                8,
                u8::try_from(i % 60).unwrap(),
            ))
        })
        .collect();
    let provider = fakes::FakeProvider::with_mail(vec![fakes::inbox()], messages);
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let view = app.attachment_browser(unfiltered()).await;

    assert_eq!(view.coverage.scanned, 200, "the scan stops at the window");
    assert_eq!(view.coverage.skipped, 200);
    assert_eq!(view.coverage.indexed, 0);
    assert_eq!(view.entries, []);
}

#[tokio::test]
async fn the_sender_filter_matches_exact_and_case_insensitive() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = browsed_mailbox(&surfaces).await;

    let view = app
        .attachment_browser(AttachmentBrowserFilter {
            sender: Some("GRACE@EXAMPLE.TEST".to_owned()),
            mime_prefix: None,
        })
        .await;

    assert_eq!(view.entries.len(), 1, "only Grace's part stands");
    assert_eq!(view.entries[0].file_name, "notes.pdf");
    assert_eq!(view.entries[0].from_address, "grace@example.test");
}

#[tokio::test]
async fn the_mime_prefix_filter_narrows_the_parts() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = browsed_mailbox(&surfaces).await;

    let view = app
        .attachment_browser(AttachmentBrowserFilter {
            sender: None,
            mime_prefix: Some("image/".to_owned()),
        })
        .await;

    let names: Vec<&str> = view.entries.iter().map(|e| e.file_name.as_str()).collect();
    assert_eq!(names, ["logo.png"], "only the image part stands");
}

#[tokio::test]
async fn a_true_guess_with_no_parts_contributes_zero_rows() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // The flag is the provider's guess: a message flagged as attached whose
    // source carries no downloadable part contributes no rows, and the scan
    // still counts it indexed (its source read).
    let plain = b"Content-Type: text/plain\r\n\r\nNo parts here.\r\n".to_vec();
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![attached(fakes::mail("m1", "inbox", "Guess"))],
        vec![("m1".to_owned(), plain)],
    );
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);
    // The cap off keeps the warm from fetching the source before the browse,
    // so the flag is still the provider's guess when the scan matches it; the
    // browse's own fetch-on-miss reads the source and finds no parts.
    app.dispatch(Intent::Settings(SettingsIntent::SetSyncSettings {
        account: AccountId::try_from("work").unwrap(),
        depth_months: None,
        body_cap: Some(SyncBodyCap::Off),
        poll_minutes: None,
    }))
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let view = app.attachment_browser(unfiltered()).await;

    assert!(
        view.entries.is_empty(),
        "a true-guess with no parts lists nothing"
    );
    assert_eq!(view.coverage.scanned, 1);
    assert_eq!(
        view.coverage.indexed, 1,
        "the source read, it carried no parts"
    );
    assert_eq!(view.coverage.skipped, 0);
}

#[tokio::test]
async fn the_browse_fetches_on_a_miss_and_reuses_the_cache_on_recompute() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // The message syncs without a source (the warm's fetch refuses), so the
    // first browse meets a cache miss: the escalated fetch-on-miss sweep.
    let provider = fakes::FakeProvider::with_mail(
        vec![fakes::inbox()],
        vec![attached(fakes::mail("m1", "inbox", "Launch"))],
    );
    let fetches = Arc::clone(&provider.fetches);
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let account = AccountId::try_from("work").unwrap();
    let handle = app.account_handle(&account).await.unwrap();
    handle
        .providers
        .first()
        .unwrap()
        .sources
        .lock()
        .unwrap()
        .insert("m1".to_owned(), mixed_source());
    let baseline = fetches.load(std::sync::atomic::Ordering::SeqCst);

    let first = app.attachment_browser(unfiltered()).await;
    let after_first = fetches.load(std::sync::atomic::Ordering::SeqCst);
    let second = app.attachment_browser(unfiltered()).await;
    let after_second = fetches.load(std::sync::atomic::Ordering::SeqCst);

    assert_eq!(
        first.entries.len(),
        2,
        "the missed source fetches and lists"
    );
    assert_eq!(first, second, "a recompute answers the same fresh rows");
    assert_eq!(after_first, baseline + 1, "the first open fetches the miss");
    assert_eq!(
        after_second,
        baseline + 1,
        "the second open reads the cache"
    );
}
