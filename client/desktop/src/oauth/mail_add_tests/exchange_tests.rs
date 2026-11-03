// SPDX-License-Identifier: GPL-3.0-only
//! The exchange half's own tests: the document a completed sign-in
//! answers (the `[imap]` section with its grant and no password, the
//! accepted certificate riding, the advertised issuer kept for the
//! redirect's `iss` check), the refusal classification, the door rule
//! that refuses a token set with no refresh token, and the CSRF check.

use super::*;

/// An exchange answer with no refresh token: the door rule's server.
fn no_refresh_answer() -> Answer {
    (
        "200 OK",
        r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT-1","scope":"offline_access"}"#
            .to_owned(),
    )
}

/// A dead-grant answer: the code is stale or the grant was revoked.
fn invalid_grant_answer() -> Answer {
    (
        "400 Bad Request",
        r#"{"error":"invalid_grant","error_description":"revoked"}"#.to_owned(),
    )
}

#[test]
fn the_exchange_answers_the_document_the_grant_belongs_to() {
    let endpoint = token_endpoint(healthy_answer);

    let document = exchange(pending(&endpoint)).expect("the exchange answers a document");

    let config = astra_account::load_str(&document).expect("the document loads");
    let imap = config.imap.expect("the imap section");
    assert_eq!(imap.addr, "mail.example.test:993");
    assert_eq!(imap.server_name, "mail.example.test");
    assert_eq!(imap.username, "alice@example.test");
    assert!(
        imap.password.is_none(),
        "a grant-carrying account stores no password"
    );
    let grant = imap.oauth.expect("the grant");
    assert_eq!(grant.client_id, "client-xyz");
    assert_eq!(grant.refresh_token.expose(), "rt-9");
    assert_eq!(grant.token_endpoint, endpoint);
    assert_eq!(
        grant.issuer.as_deref(),
        Some("https://id.example.test"),
        "the advertised issuer rides, for the redirect's iss check"
    );
    assert_eq!(
        config.certificate_exceptions.len(),
        1,
        "the certificate the person accepted rides into the stored document"
    );
}

#[test]
fn a_refused_exchange_crosses_the_classification_word() {
    let endpoint = token_endpoint(invalid_grant_answer);

    let failure = exchange(pending(&endpoint)).expect_err("the refused exchange refuses");

    assert_eq!(
        failure.refusal,
        RefusalKind::Dead,
        "an invalid_grant is the dead word"
    );
}

#[test]
fn a_token_set_with_no_refresh_token_is_refused_at_the_door() {
    // Without a refresh token the account would break within the hour,
    // which is far worse to debug than refusing here.
    let endpoint = token_endpoint(no_refresh_answer);

    let failure = exchange(pending(&endpoint)).expect_err("the door rule refuses");

    assert_eq!(failure.refusal, RefusalKind::Indeterminate);
    assert!(
        failure.detail.contains("refresh token"),
        "the door's own sentence rides: {}",
        failure.detail
    );
}

#[test]
fn a_forged_callback_is_refused() {
    let endpoint = token_endpoint(healthy_answer);
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let callback = "com.kenizhou.astra://oauth?code=abc.def&state=forged".to_owned();

    let failure = runtime
        .block_on(move || async move {
            super::super::complete_mail_login(
                &serde_json::to_string(&pending(&endpoint)).expect("the pending handle serialises"),
                &callback,
                time::OffsetDateTime::now_utc(),
            )
            .await
        })
        .expect_err("the state mismatch refuses");

    assert!(
        failure.detail.contains("state"),
        "the CSRF refusal names itself: {}",
        failure.detail
    );
}
