// SPDX-License-Identifier: GPL-3.0-only
//! The live discovery proof against the local Stalwart harness
//! (docker/harness), pinning honestly what its JMAP origin serves.
//!
//! The harness publishes Stalwart's built-in HTTP listener (JMAP plus
//! management) on `127.0.0.1:2080` as plain HTTP, and that listener
//! serves two OAuth metadata documents:
//!
//! - `/.well-known/oauth-protected-resource` answers `200` with a real
//!   RFC 9728 document naming `https://mail.example.test` as both the
//!   resource and its authorization server. The first half of the
//!   discovery chain therefore runs green live: the origin's
//!   unauthenticated GET answers a redirect with no `WWW-Authenticate`
//!   challenge, the no-follow redirect policy treats that as no pointer,
//!   and the RFC 9728 well-known fallback finds the document.
//! - `/.well-known/oauth-authorization-server` answers `200` with a real
//!   RFC 8414 document claiming the issuer `https://mail.example.test`,
//!   advertising S256 PKCE and a registration endpoint. The harness
//!   domain resolves nowhere by design (the detection suite rebases onto
//!   the origin through its override seam for exactly that reason), so
//!   the document cannot be fetched under the issuer it names: no green
//!   live `AuthServerMetadata` run exists against this harness. What is
//!   pinned live instead is the refusal: asking about the loopback origin
//!   that really served the document must fail closed on the RFC 8414
//!   section 3.3 issuer mismatch, the check that stops a compromised
//!   resource pointing this app at an attacker's token endpoint. The
//!   validated-green half of the chain stands on the offline decision
//!   tests, which hold every rule a plaintext mock would have to disable.
//!
//! The tests skip unless `ASTRA_LIVE_HARNESS=1` (the same gate the app's
//! live suites read), so the offline `cargo test` stays green. Against a
//! running harness:
//!
//! ```sh
//! scripts/dev/harness.sh up
//! eval "$(scripts/dev/harness.sh env)"
//! cargo test -p astra-oauth discovery_live -- --nocapture
//! ```

use super::*;

/// The harness's plain-HTTP JMAP origin from the environment, or `None` to
/// skip.
fn harness_origin() -> Option<String> {
    if std::env::var("ASTRA_LIVE_HARNESS").ok().as_deref() != Some("1") {
        return None;
    }
    Some(
        std::env::var("ASTRA_HARNESS_JMAP_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:2080".to_owned()),
    )
}

/// The issuer the harness's documents claim: the harness's own certificate
/// domain, which resolves nowhere outside the container.
const HARNESS_ISSUER: &str = "https://mail.example.test";

/// The protected-resource half, green: the harness origin yields a real
/// RFC 9728 document through the well-known fallback route, and the
/// document names the authorization server the second half would ask
/// about.
#[tokio::test]
async fn the_harness_origin_names_its_authorization_server_over_rfc_9728() {
    let Some(origin) = harness_origin() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let http = discovery_client().expect("the shared discovery client builds");
    let resource = discover_protected_resource(&http, &origin)
        .await
        .expect("the harness origin yields its RFC 9728 document");

    // The issuer the document names is the harness's own domain, and the
    // resource is the same origin: both are HTTPS strings, so both pass
    // the hop rule without the network ever being asked to resolve the
    // name.
    assert_eq!(resource.issuer, HARNESS_ISSUER);
    assert_eq!(
        resource.resource.as_deref(),
        Some(HARNESS_ISSUER),
        "the RFC 8707 resource the token requests must name",
    );
}

/// The authorization-server half, pinned by its boundary: the loopback
/// origin is admitted by the hop rule and Stalwart really serves the
/// RFC 8414 document there, so the fetch and the parse succeed live, and
/// the issuer check is what refuses. Any other failure would mean the
/// harness stopped serving the document this proof pins.
#[tokio::test]
async fn the_harness_metadata_document_refuses_under_a_mismatched_issuer() {
    let Some(origin) = harness_origin() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let http = discovery_client().expect("the shared discovery client builds");

    let error = discover_auth_server(&http, &origin)
        .await
        .expect_err("the document claims an issuer the origin is not");
    let DiscoveryError::IssuerMismatch { asked, claimed } = error else {
        panic!("the loopback document must fail on the issuer check, got {error}");
    };
    assert_eq!(asked, origin);
    assert_eq!(claimed, HARNESS_ISSUER);
}
