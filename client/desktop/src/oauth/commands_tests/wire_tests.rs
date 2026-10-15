// SPDX-License-Identifier: GPL-3.0-only
//! The wire literals, byte for byte the JSON the web's oauth.test.ts
//! reads, beside the completion stand-in the begin/cancel tests drive
//! their flows with: the provider words cross bare and snake_cased, the
//! successes as a bare word or an added record, the refusal as its
//! tagged shape.

use astra_oauth::{OAuthProviderConfig, TokenSet};

use super::super::{
    BrowserOpened, OAuthOfferView, ProviderKind, RefusalKind, SignInOutcome, SignInResult,
};

/// The completion stand-in: the bare word, as a provider no account
/// consumes answers with. The Microsoft completion itself is driven in
/// the account half's own tests, over its seams.
pub(super) fn bare_completion() -> impl FnOnce(
    TokenSet,
    OAuthProviderConfig,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = SignInOutcome> + Send>>
       + Send {
    |tokens, _provider| {
        Box::pin(async move {
            let _ = tokens;
            SignInOutcome::SignedIn
        })
    }
}

#[test]
fn the_views_cross_as_the_web_reads_them() {
    assert_eq!(
        serde_json::to_value(OAuthOfferView {
            providers: vec![ProviderKind::Microsoft, ProviderKind::Google],
        })
        .expect("the offer serialises"),
        serde_json::json!({"providers": ["microsoft", "google"]})
    );
    assert_eq!(
        serde_json::to_value(BrowserOpened {
            provider: ProviderKind::Microsoft
        })
        .expect("the browser-opened answer serialises"),
        serde_json::json!({"provider": "microsoft"})
    );
    assert_eq!(
        serde_json::to_value(SignInResult {
            provider: ProviderKind::Microsoft,
            outcome: SignInOutcome::SignedIn,
        })
        .expect("the success serialises"),
        serde_json::json!({"provider": "microsoft", "outcome": "signed_in"})
    );
    assert_eq!(
        serde_json::to_value(SignInResult {
            provider: ProviderKind::Microsoft,
            outcome: SignInOutcome::Added {
                address: "alice@example.com".to_owned(),
                calendar_withheld: true,
            },
        })
        .expect("the added account serialises"),
        serde_json::json!({
            "provider": "microsoft",
            "outcome": {"added": {"address": "alice@example.com", "calendar_withheld": true}}
        })
    );
    assert_eq!(
        serde_json::to_value(SignInResult {
            provider: ProviderKind::Google,
            outcome: SignInOutcome::Refused {
                refusal: RefusalKind::Dead,
                detail: "oauth endpoint error: invalid_grant; (no description)".to_owned(),
            },
        })
        .expect("the refusal serialises"),
        serde_json::json!({"provider": "google", "outcome": {"refused": {"refusal": "dead", "detail": "oauth endpoint error: invalid_grant; (no description)"}}})
    );
}
