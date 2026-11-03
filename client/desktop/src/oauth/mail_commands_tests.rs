// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the mail sign-in command surface: the probe is bounded and
//! fails soft in both directions (a setup that cannot build, a probe
//! that never answers, a server that advertises nothing: the password
//! screen, never an error), the ask runs over the step's own walk (the
//! accepted certificate pins in, a blank password never blocks it, the
//! detection's issuer crosses), and the begin runs the minted URL
//! through the session's external arm exactly the JMAP begin's shape.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use astra_account::{ConnectionSecurity, ImapAuth};

use super::imap_auth_offer_with;
use super::ImapAuthView;

/// The step's setup payload, exactly the JSON the web's buildSetup
/// crosses: the password the person may have typed so far rides in, and
/// the probe never needs it.
fn setup_json() -> serde_json::Value {
    serde_json::json!({
        "imap_host": "mail.example.test",
        "username": "alice@example.test",
        "credential": {"password": "secret"},
        "imap_security": "implicit_tls",
        "smtp_security": "implicit_tls",
    })
}

/// The same setup with a certificate the person accepted and a blank
/// password: the ask must run before anything is typed and under the
/// trust the acceptance walk built.
fn accepted_setup_json() -> serde_json::Value {
    serde_json::json!({
        "imap_host": "mail.example.test",
        "username": "alice@example.test",
        "credential": {"password": ""},
        "imap_security": "start_tls",
        "smtp_security": "start_tls",
        "accepted_certificate": {
            "server_name": "mail.example.test",
            "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        },
    })
}

/// How long a bounded test waits: enough for the runtime's spawn, small
/// enough that a hanging probe fails fast.
const SMALL_BOUND: Duration = Duration::from_millis(200);

/// What one scripted ask observed of the question it was handed.
#[derive(Default)]
struct AskSaw {
    hosts: Mutex<Vec<String>>,
    securities: Mutex<Vec<ConnectionSecurity>>,
    emails: Mutex<Vec<String>>,
    issuers: Mutex<Vec<Option<String>>>,
    exceptions: Mutex<usize>,
    answers: Mutex<usize>,
}

/// The ask seam answering `answer` while recording what the question
/// carried.
fn ask_with(
    saw: Arc<AskSaw>,
    answer: impl Fn() -> ImapAuth + Send,
) -> impl FnOnce(super::MailAuthAsk) -> std::future::Ready<ImapAuth> + Send {
    move |ask| {
        let imap = ask
            .config
            .imap
            .as_ref()
            .expect("the built config names an imap server");
        saw.hosts.lock().unwrap().push(imap.addr.clone());
        saw.securities.lock().unwrap().push(imap.security);
        saw.emails.lock().unwrap().push(imap.username.clone());
        saw.issuers.lock().unwrap().push(ask.oauth_issuer.clone());
        *saw.exceptions.lock().unwrap() += ask.config.tls_exceptions().len();
        *saw.answers.lock().unwrap() += 1;
        std::future::ready(answer())
    }
}

#[path = "mail_commands_tests/begin_tests.rs"]
mod begin_tests;

#[test]
fn the_three_answers_cross_typed_over_the_servers_own_word() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let saw = Arc::new(AskSaw::default());

    let sign_in = imap_auth_offer_with(
        &runtime,
        SMALL_BOUND,
        setup_json(),
        None,
        ask_with(Arc::clone(&saw), || ImapAuth::SignIn {
            issuer: "https://id.example.test".to_owned(),
            provider_label: None,
            password_also_works: true,
        }),
    );
    assert_eq!(
        sign_in,
        ImapAuthView::SignIn {
            issuer: "https://id.example.test".to_owned(),
            provider_label: None,
            password_also_works: true,
        },
        "the sign-in answer crosses whole: the issuer, the label, and the password's standing"
    );

    let registration = imap_auth_offer_with(
        &runtime,
        SMALL_BOUND,
        setup_json(),
        None,
        ask_with(Arc::clone(&saw), || ImapAuth::RegistrationNeeded {
            password_also_works: false,
        }),
    );
    assert_eq!(
        registration,
        ImapAuthView::RegistrationNeeded {
            password_also_works: false,
        },
        "the registration answer crosses with its own password flag"
    );

    let password = imap_auth_offer_with(
        &runtime,
        SMALL_BOUND,
        setup_json(),
        None,
        ask_with(Arc::clone(&saw), || ImapAuth::Password),
    );
    assert_eq!(
        password,
        ImapAuthView::Password,
        "the password answer crosses as the bare word"
    );
    assert_eq!(*saw.answers.lock().unwrap(), 3, "every ask ran");
}

#[test]
fn a_setup_that_cannot_build_is_the_password_screen() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let saw = Arc::new(AskSaw::default());

    let answer = imap_auth_offer_with(
        &runtime,
        SMALL_BOUND,
        serde_json::json!({"nothing": "here"}),
        None,
        ask_with(Arc::clone(&saw), || ImapAuth::Password),
    );

    assert_eq!(answer, ImapAuthView::Password);
    assert_eq!(
        *saw.answers.lock().unwrap(),
        0,
        "a setup that cannot build never asks the server"
    );
}

#[test]
fn the_ask_runs_over_the_step_s_own_walk_with_the_accepted_certificate() {
    // The ask builds its config exactly the test connection's walk, so
    // the certificate the person accepted pins into the probe's TLS; the
    // blank password never blocks the question, and the STARTTLS choice
    // and the derived dial address cross into the query.
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let saw = Arc::new(AskSaw::default());

    let answer = imap_auth_offer_with(
        &runtime,
        SMALL_BOUND,
        accepted_setup_json(),
        Some("https://id.example.test".to_owned()),
        ask_with(Arc::clone(&saw), || ImapAuth::Password),
    );

    assert_eq!(answer, ImapAuthView::Password);
    let asked = saw.answers.lock().unwrap();
    assert_eq!(*asked, 1, "the blank password did not block the ask");
    drop(asked);
    assert_eq!(
        saw.hosts.lock().unwrap().as_slice(),
        ["mail.example.test:143"],
        "the question dials the derived STARTTLS address the test connection would"
    );
    assert_eq!(
        saw.securities.lock().unwrap().as_slice(),
        [ConnectionSecurity::StartTls],
        "the security choice the person made crosses"
    );
    assert_eq!(
        saw.emails.lock().unwrap().as_slice(),
        ["alice@example.test"]
    );
    assert_eq!(
        saw.issuers.lock().unwrap().as_slice(),
        [Some("https://id.example.test".to_owned())],
        "the issuer the provider's own autoconfig named crosses into the question"
    );
    assert_eq!(
        *saw.exceptions.lock().unwrap(),
        1,
        "the accepted certificate pins into the question's config, the walk the test connection takes"
    );
}

#[test]
fn a_probe_that_never_answers_is_bounded_and_fails_soft() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");

    let answer = imap_auth_offer_with(&runtime, SMALL_BOUND, setup_json(), None, |_ask| async {
        let () = std::future::pending().await;
        unreachable!("the hanging ask never answers")
    });

    assert_eq!(
        answer,
        ImapAuthView::Password,
        "a bound-out question is the password screen, never an error"
    );
}
