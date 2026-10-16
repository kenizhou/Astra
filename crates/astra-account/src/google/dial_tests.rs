// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the Google account dial: the one function every path that
//! brings a Google account up calls (the boot, the sign-in's
//! completion). The healthy dial needs Google's own endpoints (the
//! calendar-list read), so the offline proof here is the dead-grant leg
//! both halves share through the one token source; the healthy leg runs
//! over the scripted dial seam in the desktop suite.

use std::sync::atomic::Ordering;

use engine_api::AccountId;

use crate::google::test_support::google_source_at;
use crate::graph::token_source::test_support::dead_grant_token_endpoint;
use crate::{dial_google_account, load_str, AccountError};

/// A stored Google account: the google section alone, with no imap or
/// smtp section, which is the shape a Google account's document takes.
const GOOGLE_DOC: &str = r#"
[google]
email = "alice@gmail.example"
client_id = "google-client"
client_secret = "gsecret"
redirect_uri = "http://127.0.0.1:45001/"
scopes = ["https://mail.google.com/"]
refresh_token = "initial-refresh"
"#;

#[tokio::test]
async fn a_dial_of_a_dead_grant_fails_after_exactly_one_refresh() {
    let (endpoint, hits) = dead_grant_token_endpoint();
    let source = google_source_at(endpoint, Some("gsecret"), None);
    let account = AccountId::try_from("alice@gmail.example@google").unwrap();
    let config = load_str(GOOGLE_DOC).expect("a stored google account");

    let err = dial_google_account(&account, source, &config)
        .await
        .unwrap_err();
    // The mail leg is the account's: a grant the server refused fails
    // the dial as a refused sign-in, which a caller answers with a
    // re-auth rather than an outage badge.
    assert!(
        matches!(err, AccountError::SigninRejected(_)),
        "the dial reported an outage for a grant the server refused: {err}",
    );
    // The mail probe and the calendar list run concurrently over the one
    // token source, so one refresh serves both: the second caller takes
    // the shared failure memo instead of presenting the same refresh
    // token again.
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "the dial presented a refresh token twice",
    );
}
