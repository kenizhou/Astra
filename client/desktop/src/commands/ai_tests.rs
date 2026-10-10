// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the AI replies commands: the chips pull round trips through
//! the fake app and a dismissal hides them, the summarise pull answers the
//! string and maps its failures onto the command error, the style pull
//! answers the note, and the chip's wire shape is the two fields the
//! reading pane renders. The fake app and the state builder live in
//! `commands_fakes`, one compiled copy for every command test module.

use std::sync::Arc;

use astra_app::Chip;
use astra_viewmodel::ComposeKind;

use super::{
    chips_dismiss_of, compose_autocomplete_of, compose_from_draft_of, draft_chips_of,
    knowledge_extract_of, knowledge_forget_of, knowledge_list_of, snippet_list_of,
    snippet_search_of, style_profile_of, summarise_of, translate_of,
};
use crate::commands::compose::compose_request_of;
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

#[test]
fn the_accept_hands_the_draft_through_and_the_core_publishes_the_reply() {
    let app = FakeApp::new();
    let state = state("ai-accept", Arc::clone(&app), FakeStore::accepting());
    let (account, key) = key_parts();

    state
        .runtime
        .block_on(|| compose_from_draft_of(&state, &account, &key, "Sounds good, shipped it."));

    assert_eq!(
        app.drafted_into.lock().unwrap().as_slice(),
        [(
            "work".to_owned(),
            "imap:v1:u42".to_owned(),
            "Sounds good, shipped it.".to_owned()
        )]
        .as_slice(),
        "the command hands the webview's account, key, and drafted reply to the core",
    );
    let request = compose_request_of(&state).expect("the accept published the reply's request");
    assert_eq!(
        request.kind,
        ComposeKind::Reply,
        "the composition a chip opens is the reply the reader accepted"
    );
}

#[test]
fn the_knowledge_listing_round_trips_and_the_forget_removes_one_row() {
    let app = FakeApp::new();
    app.holding_knowledge("k1", "fact", "The launch lands on Friday.");
    app.holding_knowledge("k2", "preference", "The user prefers plain text.");
    let state = state("ai-knowledge", Arc::clone(&app), FakeStore::accepting());
    let (account, _) = key_parts();

    let listed = knowledge_list_of(&state, &account);
    assert_eq!(
        listed
            .iter()
            .map(|entry| entry.id.as_str().to_owned())
            .collect::<Vec<_>>(),
        vec!["k1".to_owned(), "k2".to_owned()],
        "the listing answers the core's rows in their own order",
    );
    assert_eq!(
        app.assist.lock().unwrap().knowledge_into.as_slice(),
        ["work".to_owned()].as_slice(),
        "the command hands the webview's account to the core",
    );

    assert!(
        knowledge_forget_of(&state, "k1"),
        "the forget answers the row it removed",
    );
    assert!(
        !knowledge_forget_of(&state, "k1"),
        "a second forget of the same id answers false",
    );
    assert_eq!(
        app.assist.lock().unwrap().forgotten_into.as_slice(),
        ["k1".to_owned(), "k1".to_owned()].as_slice(),
        "the ids crossed as the strings the webview sent",
    );
    assert_eq!(
        knowledge_list_of(&state, &account).len(),
        1,
        "only the forgotten row went",
    );
}

#[test]
fn the_extraction_pull_crosses_and_defaults_its_limit() {
    let app = FakeApp::new();
    app.assist.lock().unwrap().extraction = astra_app::ExtractionSummary {
        extracted: 2,
        skipped_duplicates: 1,
        messages_considered: 3,
    };
    let state = state("ai-extract", Arc::clone(&app), FakeStore::accepting());
    let (account, _) = key_parts();

    let summary = state
        .runtime
        .block_on(|| knowledge_extract_of(&state, &account, None));
    assert_eq!(
        summary,
        astra_app::ExtractionSummary {
            extracted: 2,
            skipped_duplicates: 1,
            messages_considered: 3,
        },
        "the run's summary crosses whole",
    );
    let capped = state
        .runtime
        .block_on(|| knowledge_extract_of(&state, &account, Some(10)));
    assert_eq!(capped.extracted, 2);
    assert_eq!(
        app.assist.lock().unwrap().extracted_into.as_slice(),
        [
            ("work".to_owned(), astra_app::KNOWLEDGE_EXTRACT_LIMIT),
            ("work".to_owned(), 10)
        ]
        .as_slice(),
        "an absent limit crosses as the thirty cap and a named one as itself",
    );
}

#[test]
fn the_snippet_listing_and_search_round_trip_the_library() {
    let app = FakeApp::new();
    app.holding_snippet("s1", "Greeting", "Thank you for the note.");
    app.holding_snippet("s2", "Invoice chase", "Attached is the invoice again.");
    let state = state("ai-snippets", Arc::clone(&app), FakeStore::accepting());

    let listed = snippet_list_of(&state);
    assert_eq!(
        listed
            .iter()
            .map(|snippet| snippet.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Greeting", "Invoice chase"],
    );
    let found = state
        .runtime
        .block_on(|| snippet_search_of(&state, "the money one"));
    assert_eq!(found.len(), 2, "the search answers the core's ranking");
    assert_eq!(
        app.assist.lock().unwrap().searched_into.as_slice(),
        ["the money one".to_owned()].as_slice(),
        "the search text crossed untouched",
    );
}

#[test]
fn the_autocomplete_pull_crosses_both_halves_and_an_absent_trailing_half() {
    let app = FakeApp::new();
    app.assist.lock().unwrap().suggestion = "shipped it.".to_owned();
    let state = state("ai-autocomplete", Arc::clone(&app), FakeStore::accepting());

    let suggestion = state
        .runtime
        .block_on(|| compose_autocomplete_of(&state, "Thanks for the", Some(" soon.")));
    assert_eq!(suggestion, "shipped it.");
    let tailless = state
        .runtime
        .block_on(|| compose_autocomplete_of(&state, "Thanks for the", None));
    assert_eq!(tailless, "shipped it.");
    assert_eq!(
        app.assist.lock().unwrap().suggested_into.as_slice(),
        [
            ("Thanks for the".to_owned(), " soon.".to_owned()),
            ("Thanks for the".to_owned(), String::new())
        ]
        .as_slice(),
        "an absent trailing half crosses as the empty string",
    );
}

#[test]
fn the_translate_pull_crosses_the_reference_and_the_language_word() {
    let app = FakeApp::new();
    app.assist.lock().unwrap().translation = "Corps du message.".to_owned();
    let state = state("ai-translate", Arc::clone(&app), FakeStore::accepting());
    let (account, key) = key_parts();

    let translation = state
        .runtime
        .block_on(|| translate_of(&state, &account, &key, astra_app::TranslateLanguage::French));
    assert_eq!(translation, "Corps du message.");
    assert_eq!(
        app.assist.lock().unwrap().translated_into.as_slice(),
        [(
            "work".to_owned(),
            "imap:v1:u42".to_owned(),
            "french".to_owned()
        )]
        .as_slice(),
        "the language crossed as its snake_case wire word",
    );
}

#[test]
fn the_extraction_summary_crosses_as_its_three_wire_fields() {
    let summary = astra_app::ExtractionSummary {
        extracted: 2,
        skipped_duplicates: 1,
        messages_considered: 3,
    };
    let wire = serde_json::to_value(summary).expect("the summary serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "extracted": 2,
            "skipped_duplicates": 1,
            "messages_considered": 3
        }),
    );
}

#[test]
fn the_snippet_crosses_as_its_five_wire_fields() {
    let snippet = astra_app::Snippet {
        id: astra_ai::SnippetId::new("s1".to_owned()),
        title: "Greeting".to_owned(),
        body: "Thank you for the note.".to_owned(),
        created_at: 1,
        updated_at: 2,
    };
    let wire = serde_json::to_value(&snippet).expect("the snippet serialises");
    assert_eq!(
        wire,
        serde_json::json!({
            "id": "s1",
            "title": "Greeting",
            "body": "Thank you for the note.",
            "created_at": 1,
            "updated_at": 2
        }),
    );
}
