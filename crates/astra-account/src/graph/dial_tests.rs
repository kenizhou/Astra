// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the Microsoft account dial: the one function every path that
//! brings a Graph account up calls (the boot, the sign-in's completion, a
//! reconnect).

use std::sync::atomic::Ordering;

use engine_api::{AccountId, TimeZoneId};

use crate::graph::token_source::test_support::{dead_grant_token_endpoint, source_at};
use crate::{dial_microsoft_account, load_str, AccountError};

/// A stored Microsoft account: the graph section alone, with no imap or
/// smtp section, which is the shape a Graph account's document takes.
const GRAPH_DOC: &str = r#"
[graph]
email = "alice@example.com"
client_id = "client-abc"
tenant = "common"
redirect_uri = "com.kenizhou.astra://oauth"
scopes = ["offline_access"]
refresh_token = "initial-refresh"
"#;

#[tokio::test]
async fn a_dial_of_a_dead_grant_fails_on_its_mail_leg_after_exactly_one_refresh() {
    let (endpoint, hits) = dead_grant_token_endpoint();
    let source = source_at(endpoint, None);
    let account = AccountId::try_from("alice@example.com@graph.microsoft.com").unwrap();
    let config = load_str(GRAPH_DOC).expect("a stored graph account");

    let err = dial_microsoft_account(&account, source, &config, TimeZoneId::utc())
        .await
        .unwrap_err();
    // The mail leg is the account's: a grant the server refused fails the dial as a
    // refused sign-in, which a caller answers with a re-auth rather than an outage
    // badge.
    assert!(
        matches!(err, AccountError::SigninRejected(_)),
        "the dial reported an outage for a grant the server refused: {err}",
    );
    // The mail and calendar probes run concurrently over the one token source, so one
    // refresh serves both: the second caller takes the shared failure memo instead of
    // presenting the same refresh token again.
    assert_eq!(
        hits.load(Ordering::SeqCst),
        1,
        "the dial presented a refresh token twice",
    );
}
