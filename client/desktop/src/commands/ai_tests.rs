// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the AI replies commands: the chips pull round trips through
//! the fake app and a dismissal hides them, the summarise pull answers the
//! string and maps its failures onto the command error, the style pull
//! answers the note, and the chip's wire shape is the two fields the
//! reading pane renders. The fake app and the state builder live in
//! `commands_fakes`, one compiled copy for every command test module.

use std::sync::Arc;

use astra_app::Chip;

use super::{chips_dismiss_of, draft_chips_of, style_profile_of, summarise_of};
use crate::commands::CommandError;
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The fixture message the AI pulls name: the same account and provider
/// key the reading fixtures use.
fn key_parts() -> (engine_api::AccountId, engine_api::ProviderKey) {
    (
        engine_api::AccountId::try_from("work").expect("an account id"),
        engine_api::ProviderKey::new("imap:v1:u42").expect("a provider key"),
    )
}

/// Seeds the fake's chips for the fixture message.
fn seed_chips(app: &FakeApp) {
    *app.chips.lock().unwrap() = vec![(
        ("work".to_owned(), "imap:v1:u42".to_owned()),
        vec![Chip {
            reply: "Sounds good, shipped it.".to_owned(),
            preview: "Sounds good, shipped it.".to_owned(),
        }],
    )];
}

#[test]
fn the_chips_pull_round_trips_and_a_dismissal_hides_the_chips() {
    let app = FakeApp::new();
    seed_chips(&app);
    let state = state("ai-chips", Arc::clone(&app), FakeStore::accepting());
    let (account, key) = key_parts();

    let chips = state
        .runtime
        .block_on(|| draft_chips_of(&state, &account, &key));
    assert_eq!(
        chips,
        vec![Chip {
            reply: "Sounds good, shipped it.".to_owned(),
            preview: "Sounds good, shipped it.".to_owned(),
        }],
        "the pull answers the core's chips",
    );
    assert_eq!(
        app.chips_into.lock().unwrap().as_slice(),
        [("work".to_owned(), "imap:v1:u42".to_owned())].as_slice(),
        "the command hands the webview's account and key to the core",
    );

    chips_dismiss_of(&state, &account, &key);
    assert_eq!(
        app.dismissed.lock().unwrap().as_slice(),
        [("work".to_owned(), "imap:v1:u42".to_owned())].as_slice(),
        "the dismiss write reached the core",
    );
    assert!(
        state
            .runtime
            .block_on(|| draft_chips_of(&state, &account, &key))
            .is_empty(),
        "a dismissed message surfaces no chips",
    );
}

#[test]
fn the_summarise_pull_answers_the_string_and_maps_its_failures() {
    let app = FakeApp::new();
    *app.summary.lock().unwrap() = Some("The launch is ready.".to_owned());
    let state = state("ai-summarise", Arc::clone(&app), FakeStore::accepting());
    let (account, key) = key_parts();

    let summary = state
        .runtime
        .block_on(|| summarise_of(&state, &account, &key))
        .expect("the pull answers");
    assert_eq!(summary, "The launch is ready.");
    assert_eq!(
        app.summarise_into.lock().unwrap().as_slice(),
        [("work".to_owned(), "imap:v1:u42".to_owned())].as_slice(),
    );

    *app.summary.lock().unwrap() = None;
    let refused = state
        .runtime
        .block_on(|| summarise_of(&state, &account, &key))
        .expect_err("the unresolvable message refuses");
    assert_eq!(
        refused.to_string(),
        CommandError::Ai("the message could not be resolved for summarising".to_owned())
            .to_string(),
        "the core's failure maps onto the command error the webview sees",
    );
}

#[test]
fn the_style_pull_answers_the_account_note_or_null() {
    let app = FakeApp::new();
    *app.style_note.lock().unwrap() = Some("Crisp and warm.".to_owned());
    let state = state("ai-style", Arc::clone(&app), FakeStore::accepting());
    let (account, _) = key_parts();

    let note = state
        .runtime
        .block_on(|| style_profile_of(&state, &account));
    assert_eq!(note.as_deref(), Some("Crisp and warm."));
    assert_eq!(
        app.style_into.lock().unwrap().as_slice(),
        ["work".to_owned()].as_slice(),
    );

    *app.style_note.lock().unwrap() = None;
    assert_eq!(
        state
            .runtime
            .block_on(|| style_profile_of(&state, &account)),
        None,
        "no profile crosses as null",
    );
}

#[test]
fn the_chip_crosses_as_its_two_wire_fields() {
    let chip = Chip {
        reply: "Sounds good, shipped it.".to_owned(),
        preview: "Sounds good...".to_owned(),
    };
    let wire = serde_json::to_value(&chip).expect("the chip serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "reply": "Sounds good, shipped it.",
            "preview": "Sounds good..."
        }),
    );
}
