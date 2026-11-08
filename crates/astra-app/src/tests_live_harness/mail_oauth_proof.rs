// SPDX-License-Identifier: GPL-3.0-only
//! The gated mail OAuth proofs against the live harness: the wizard's
//! question answered from the server's own capability line on both legs,
//! and the discovery's fail-soft over a domain that resolves nowhere.
//!
//! What only a live server can show is that the answer survives the
//! dial. The offline suites pin the classification over scripted
//! capability lines; the harness's Stalwart is a real server whose
//! pre-auth line (AUTH=PLAIN beside AUTH=OAUTHBEARER and AUTH=XOAUTH2 on
//! the IMAP leg; the EHLO AUTH line naming the same set on the
//! submission leg) must cross the engine's real probe verbs and the
//! account layer's real decision. The IMAP leg rides the product's own
//! TLS acceptance walk, because the probe must gate on the dialled
//! server, never a second dial: the certificate is refused, recorded,
//! and accepted for that one server, and the same accepted certificate
//! serves both legs. The harness offers OAuth mechanisms but serves no
//! issuer (the JMAP slice's finding: example.test resolves nowhere), so
//! the composed answer is the honest middle shape, RegistrationNeeded
//! with the password still working.
//!
//! The discovery proof pins the fail-soft the same server serves: the
//! issuer chain over the harness's domain answers None within its
//! budget (no autoconfig oAuth2 in the detected shape, both well-known
//! candidates resolving nowhere), and the wizard's ask run the way the
//! product runs it before any certificate is accepted answers Password:
//! a probe whose dial the un-accepted self-signed certificate refuses
//! never surfaces an error, and the password field, which works
//! everywhere, stands. A full sign-in round trip needs an issuer the
//! harness does not serve; that proof waits on a real identity provider
//! exactly as JMAP's does, and the offline suites carry the complete
//! chain over scripted servers until then.

use std::time::Duration;

use provider_imap::{probe_imap_auth, probe_smtp_auth, AuthOffer, ImapSecurity};

use astra_account::{account_tls, imap_auth_offer, mail_oauth_issuer, ImapAuth, ImapAuthQuery};

use super::{dial_with_accepted_certificate, harness, harness_smtp_port};

/// The issuer search's own bound, the decision's constant: the proof
/// asserts the chain answers (with its miss) inside it, never hanging on
/// a candidate that accepts a connection and says nothing.
const ISSUER_BUDGET: Duration = Duration::from_secs(6);

/// Whether `offer` names `mechanism`, case-insensitively: mechanism
/// names are protocol atoms.
fn offers(offer: &AuthOffer, mechanism: &str) -> bool {
    offer
        .mechanisms
        .iter()
        .any(|name| name.eq_ignore_ascii_case(mechanism))
}

/// The harness's real capability line, pinned on both legs through the
/// engine's own probe verbs, with the wizard's question answered over
/// the dialled server through the product's TLS acceptance walk: the
/// server offers OAuth mechanisms beside the password on IMAP and on
/// submission, and because no issuer resolves for its domain the
/// decision composes into the registration-needed shape with the
/// password still working.
#[tokio::test]
async fn the_capability_line_pins_the_oauth_offered_shape_on_both_legs() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };

    // The wizard's own acceptance walk: the self-signed certificate is
    // refused, recorded, and accepted for the one server, and the
    // accepted config is what the ask builds its trust from, exactly the
    // shape the probe command pins the accepted certificate into.
    let (_account, _providers, config, _connections) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;
    let tls = account_tls(&config).expect("the accepted certificate builds the account tls");
    let imap = config
        .imap
        .as_ref()
        .expect("the harness account names its imap server");

    // The wizard's question over the real dialled server: the harness
    // advertises the OAuth mechanisms, no route registers, and the
    // honest middle answer stands with the password beside it.
    let detected = astra_account::detected_issuer(None);
    let query = ImapAuthQuery {
        imap_host: imap.addr.clone(),
        imap_security: imap.security,
        email: imap.username.clone(),
        detected: Some(&detected),
    };
    let answer = imap_auth_offer(&query, &tls).await;
    assert_eq!(
        answer,
        ImapAuth::RegistrationNeeded {
            password_also_works: true,
        },
        "the harness offers OAuth with no issuer, so the middle screen stands"
    );

    // The IMAP leg's own line, read by the engine's verb under the
    // accepted trust: Stalwart advertises the password mechanism beside
    // both OAuth mechanisms on the implicit TLS listener.
    let imap_offer = probe_imap_auth(
        &imap.addr,
        &imap.server_name,
        ImapSecurity::ImplicitTls,
        &tls.connector(),
    )
    .await
    .expect("the imap probe reads the harness capability line");
    assert!(imap_offer.oauth, "the imap leg offers an OAuth mechanism");
    assert!(imap_offer.password, "the imap leg offers a password");
    assert!(
        offers(&imap_offer, "OAUTHBEARER"),
        "imap line: {imap_offer:?}"
    );
    assert!(offers(&imap_offer, "XOAUTH2"), "imap line: {imap_offer:?}");
    assert!(offers(&imap_offer, "PLAIN"), "imap line: {imap_offer:?}");

    // The submission leg's own EHLO AUTH line, the same accepted
    // certificate covering the dial, introducing itself with the
    // account's mail domain the way a real submission would.
    let domain = user.rsplit_once('@').map_or("localhost", |(_, d)| d);
    let smtp_addr = format!("{host}:{}", harness_smtp_port());
    let smtp_offer = probe_smtp_auth(
        &smtp_addr,
        &imap.server_name,
        ImapSecurity::ImplicitTls,
        domain,
        &tls.connector(),
    )
    .await
    .expect("the smtp probe reads the harness EHLO auth line");
    assert!(
        smtp_offer.oauth,
        "the submission leg offers an OAuth mechanism"
    );
    assert!(smtp_offer.password, "the submission leg offers a password");
    assert!(
        offers(&smtp_offer, "OAUTHBEARER"),
        "smtp line: {smtp_offer:?}"
    );
    assert!(offers(&smtp_offer, "XOAUTH2"), "smtp line: {smtp_offer:?}");
    assert!(offers(&smtp_offer, "PLAIN"), "smtp line: {smtp_offer:?}");
}

/// The discovery's fail-soft over the harness: the issuer chain over the
/// harness's domain, asked with the detection shape its own autoconfig
/// leaves carrying no issuer, answers None within its budget; and the
/// wizard's ask, run the way the product runs it before any certificate
/// is accepted, answers Password: the refused dial is a miss like any
/// other, the decision never fails, and the password field stands.
#[tokio::test]
async fn the_discovery_fails_soft_when_no_issuer_resolves() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let domain = user.rsplit_once('@').map_or("", |(_, d)| d);

    // The chain itself, bounded by its own budget: the domain resolves
    // nowhere and the detected shape names no issuer, so both well-known
    // candidates miss and the answer is the miss, not an error.
    let detected = astra_account::detected_issuer(None);
    let found = tokio::time::timeout(ISSUER_BUDGET, mail_oauth_issuer(domain, Some(&detected)))
        .await
        .expect("the chain answers its miss within its own budget");
    assert!(
        found.is_none(),
        "the harness domain resolves nowhere; no authorization server may answer"
    );

    // The composed fail-soft over the same server, asked the way the
    // wizard's first ask runs: no certificate accepted yet, so the
    // probe's dial is refused by the self-signed certificate and the
    // answer is the password screen, never an error.
    let config = super::account(&host, &port, &user, &password, Vec::new());
    let tls = account_tls(&config).expect("the setup trust policy builds");
    let imap = config
        .imap
        .as_ref()
        .expect("the harness account names its imap server");
    let query = ImapAuthQuery {
        imap_host: imap.addr.clone(),
        imap_security: imap.security,
        email: imap.username.clone(),
        detected: Some(&detected),
    };
    let answer = imap_auth_offer(&query, &tls).await;
    assert_eq!(
        answer,
        ImapAuth::Password,
        "a probe whose dial the un-accepted certificate refuses fail-softs to the password field"
    );
}
