// SPDX-License-Identifier: GPL-3.0-only
//! The gated outage proof against the live harness: the classification
//! proven over a real refused dial, the clearing proven over the real
//! dial's own pass.
//!
//! The shared harness must never be stopped, so the refused leg points the
//! account's config at a loopback port that was bound and released: the
//! product's own watch-opening dial meets a refused connection, the real
//! error classifies as the retryable network failure the outage counts,
//! and the loop's own open fold (a watch that could not be opened records
//! a network failure) is mirrored call for call through the core's
//! recorded outcome fold. Two refused passes put the only dialled account
//! in outage, the banner answers, and the real dial's own successful pass
//! through the app's ordinary fold clears both. The state machine between
//! (the backoff, the reconnect's sync-before-rewatch) is the fake clock's,
//! proven offline over the desktop host's scripted driver.

use std::sync::Arc;

use astra_account::{AccountError, ImapConnections};
use engine_api::{EmailAddress, Engine, FailureClass};

use crate::{Account, App, AppObserver, Intent, PassOutcome};

use super::{account, dial_with_accepted_certificate, harness, CountingObserver};

/// A port on the loopback that nothing answers: bound to pick a free port
/// and released, so the next dial to it meets a refused connection.
fn refused_port() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free loopback port");
    let port = listener.local_addr().expect("the bound address").port();
    drop(listener);
    port.to_string()
}

/// The outage proof against the live harness: two passes refused by a real
/// dead port put the only dialled account in outage and raise the banner,
/// and the real dial's own successful pass clears both.
#[tokio::test]
async fn two_refused_passes_put_the_account_in_outage_and_a_real_pass_clears_it() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };

    // The only dialled account: astra1, dialed for real.
    let (account_id, providers, ..) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account_id.clone(),
            providers,
            identity: EmailAddress::new("astra1@example.test"),
        }],
        None,
        observer,
    );

    // The refused leg: the same account pointing elsewhere, a loopback port
    // nothing answers, dialed through the product's own watch opening (the
    // opening the loop's open arm drives).
    let refused = refused_port();
    let dead = account(&host, &refused, &user, &password, Vec::new());
    let connections = ImapConnections::new();
    for refused_pass in 0..2 {
        let answer = astra_account::open_watch(&connections, &dead).await;
        let Err(err) = answer else {
            panic!("refused pass {refused_pass}: a refused port answers no watch face");
        };
        let AccountError::Imap(err) = err else {
            panic!("refused pass {refused_pass}: the refusal is the transport's, not {err}");
        };
        assert_eq!(
            err.failure_class(),
            FailureClass::Retryable,
            "the real refused dial classifies as the retryable failure the outage counts"
        );
        // The loop's open fold: a watch that could not be opened records
        // the network failure, the typed call every pass path joins.
        app.record_pass_outcome(&account_id, PassOutcome::NetworkFailure);
    }

    assert!(
        app.account_in_outage(&account_id),
        "two refused passes put the account in outage"
    );
    assert!(
        app.offline_banner(),
        "the banner answers when the only dialled account is out"
    );
    let delivery = app.sync_progress().delivery;
    assert!(
        delivery.offline_banner,
        "the snapshot carries the banner answer"
    );
    assert_eq!(
        delivery.outages,
        vec![account_id.clone()],
        "the snapshot names the outaged account"
    );

    // The real dial restored: one successful pass through the app's own
    // fold clears both.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        !app.account_in_outage(&account_id),
        "the successful pass clears the badge"
    );
    assert!(
        !app.offline_banner(),
        "the successful pass clears the banner"
    );
    let delivery = app.sync_progress().delivery;
    assert!(
        !delivery.offline_banner,
        "the snapshot carries the clearing"
    );
    assert!(
        delivery.outages.is_empty(),
        "the snapshot holds no outaged account"
    );
}
