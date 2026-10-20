// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the mailto host leg: the delivery filter (only the mailto
//! scheme's URL, handed on whole; a delivery without one answers
//! nothing), the open seam (the app core's prefill emitted, the raw link
//! handed through unparsed, a refusal silent), the emission's wire
//! shape, and the reading pane's ask payload. No test touches the
//! plugin, a window, or a network: the seams the wiring composes are
//! pinned here, and the composition itself is the registration the setup
//! hook calls once.

use std::sync::{Arc, Mutex};

use astra_viewmodel::MailtoOpen;

use super::{mailto_link, open_and_emit, MailtoRequest};

/// One delivery as the plugin hands it over: every raw URL parsed.
fn delivery(raw: &[&str]) -> Vec<url::Url> {
    raw.iter()
        .map(|link| link.parse::<url::Url>())
        .collect::<Result<_, _>>()
        .expect("the delivery's urls parse")
}

/// The open a seeded core answers with: the default send account, one
/// recipient, everything else empty.
fn to_address_only() -> MailtoOpen {
    MailtoOpen {
        from: engine_api::AccountId::try_from("work").expect("the account id parses"),
        to: "ada@example.test".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: String::new(),
        body: String::new(),
    }
}

#[test]
fn the_filter_keeps_only_the_mailto_url_whole() {
    let urls = delivery(&[
        "mailto:ada@example.test?subject=Lunch",
        "com.kenizhou.astra://oauth?code=x",
    ]);
    assert_eq!(
        mailto_link(&urls).as_deref(),
        Some("mailto:ada@example.test?subject=Lunch"),
        "the mailto url is handed on whole, byte for byte"
    );
}

#[test]
fn a_delivery_without_a_mailto_url_answers_nothing() {
    let urls = delivery(&[
        "https://astra.example/compose",
        "com.kenizhou.astra://oauth?code=x",
    ]);
    assert!(
        mailto_link(&urls).is_none(),
        "a non-mailto deep link is ignored, so nothing opens"
    );
}

#[test]
fn a_mailto_with_only_a_to_address_emits_the_prefilled_open() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let open = to_address_only();
            let app = crate::commands_fakes::FakeApp::new();
            *app.mailto_open.lock().unwrap() = Some(open);
            let asked: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
            let seen = Arc::clone(&asked);

            open_and_emit(
                &(Arc::clone(&app) as Arc<dyn crate::app_state::AppPort>),
                "mailto:ada@example.test",
                move |open| {
                    seen.lock().unwrap().push(open.to);
                },
            )
            .await;

            assert_eq!(
                asked.lock().unwrap().as_slice(),
                ["ada@example.test"],
                "the core's prefill is emitted, the one recipient it carries"
            );
            assert_eq!(
                app.mailto_into.lock().unwrap().as_slice(),
                ["mailto:ada@example.test"],
                "the raw link crosses to the core whole, never parsed by the host"
            );
        });
}

#[test]
fn an_app_with_no_account_to_send_from_emits_nothing() {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| async {
            let app = crate::commands_fakes::FakeApp::new();
            let emissions = Arc::new(Mutex::new(0));
            let seen = Arc::clone(&emissions);

            open_and_emit(
                &(Arc::clone(&app) as Arc<dyn crate::app_state::AppPort>),
                "mailto:ada@example.test",
                move |_| {
                    *seen.lock().unwrap() += 1;
                },
            )
            .await;

            assert_eq!(
                *emissions.lock().unwrap(),
                0,
                "a refused open (no account stands) emits nothing"
            );
        });
}

#[test]
fn the_emission_carries_the_prefill_s_wire_shape() {
    let payload = serde_json::to_value(to_address_only()).expect("the prefill serialises");
    assert_eq!(
        payload,
        serde_json::json!({
            "from": "work",
            "to": "ada@example.test",
            "cc": "",
            "bcc": "",
            "subject": "",
            "body": "",
        }),
        "the event carries the five fields under the account, nothing else"
    );
}

#[test]
fn the_reading_pane_s_ask_carries_the_raw_link_alone() {
    let ask = serde_json::from_str::<MailtoRequest>(r#"{"uri":"mailto:ada@example.test"}"#)
        .expect("the ask parses");
    assert_eq!(ask.uri, "mailto:ada@example.test");
}
