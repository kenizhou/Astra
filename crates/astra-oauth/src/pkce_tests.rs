// SPDX-License-Identifier: GPL-3.0-only
//! PKCE shape tests: the verifier's length and charset, the S256 challenge
//! derivation, and the verifier's redaction in `Debug`.

use super::*;

#[test]
fn verifier_is_a_valid_length_and_challenge_is_the_s256_of_it() {
    let pkce = Pkce::generate();
    // 32 bytes base64url-no-pad is 43 chars; inside RFC 7636's 43..=128.
    assert_eq!(pkce.verifier().len(), 43);
    assert!((43..=128).contains(&pkce.verifier().len()));

    // The challenge is base64url(SHA-256(verifier)); recompute independently.
    let expected =
        URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, pkce.verifier().as_bytes()));
    assert_eq!(pkce.challenge(), expected);
    // A 256-bit digest base64url-no-pad is 43 chars.
    assert_eq!(pkce.challenge().len(), 43);
}

#[test]
fn from_verifier_recomputes_the_same_challenge() {
    let original = Pkce::generate();
    let rebuilt = Pkce::from_verifier(original.verifier().to_owned());
    assert_eq!(original.challenge(), rebuilt.challenge());
}

#[test]
fn generate_is_unique_per_call() {
    // Two calls must not collide: that uniqueness is what a CSPRNG verifier is for.
    assert_ne!(Pkce::generate().verifier(), Pkce::generate().verifier());
    assert_ne!(random_state(), random_state());
}

#[test]
fn debug_never_leaks_the_verifier() {
    let pkce = Pkce::generate();
    assert!(!format!("{pkce:?}").contains(pkce.verifier()));
}

#[test]
fn base64url_is_url_safe_and_unpadded() {
    // No `+`, `/`, or `=`; safe to drop straight into a URL query value.
    let state = random_state();
    assert!(!state.contains(['+', '/', '=']));
}
