// SPDX-License-Identifier: GPL-3.0-only
//! The door rules: the empty grant refused before anything registers,
//! and the refused rotation on a re-consent keeping the standing
//! document.

use super::*;

#[test]
fn an_empty_grant_is_refused_at_the_door() {
    // A grant naming no scope at all cannot bear mail: the refusal lands
    // on the setup surface the person is still looking at, and nothing
    // registers, dials, or persists (mail-calendar's door rule).
    let host = Host::new("google-add-empty-grant");
    let wiring = host.wiring();
    let mut provider = provider_config();
    provider.scopes = Vec::new();
    let mut tokens = token_set();
    tokens.scope = String::new();
    let failure = host
        .runtime
        .block_on(move || {
            complete_google_signin(
                wiring,
                tokens,
                provider,
                lookup_seam("alice@example.test"),
                ok_dial(Arc::new(DialSaw::default())),
                production_google_source(),
            )
        })
        .expect_err("the empty grant is refused before anything registers");

    assert_eq!(failure.refusal, RefusalKind::Indeterminate);
    assert!(
        host.dials.ids().is_empty(),
        "nothing registered for the empty grant"
    );
    assert!(
        host.store
            .stored(&google_id("alice@example.test"))
            .is_none(),
        "the store holds no orphaned grant"
    );
}

#[test]
fn a_refused_rotation_on_a_reconsent_keeps_the_standing_document() {
    // The re-consent's persist goes through rotate; a refused rotation
    // leaves the standing entry and the store's old document exactly as
    // they were.
    let host = Host {
        store: FakeStore::refusing(),
        ..Host::new("google-add-reconsent-refused")
    };
    register_stored(&host, stored_config(), "Alice@Example.com");

    let failure = complete(
        &host,
        lookup_seam("Alice@Example.com"),
        ok_dial(Arc::new(DialSaw::default())),
        production_google_source(),
    )
    .expect_err("the refused rotation ends the re-consent");

    assert_eq!(failure.refusal, RefusalKind::Indeterminate);
    let (standing, _) = host
        .dials
        .of(&google_id("Alice@Example.com"))
        .expect("the standing entry keeps its document after the refused rotation");
    assert_eq!(
        standing
            .google
            .as_ref()
            .and_then(|g| g.granted_scopes.clone()),
        stored_config()
            .google
            .as_ref()
            .and_then(|g| g.granted_scopes.clone()),
        "the standing document is untouched by the refused rotation"
    );
}
