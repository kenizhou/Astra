// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the people commands: the snapshot pull round trips the core's
//! published view whole, the card seed threads the reference through to the
//! core and answers the editor's fields, and the suggestions pull hands the
//! completion token and its limit down untouched.

use std::{collections::BTreeSet, sync::Arc};

use astra_app::{AvatarView, ContactDraftInput, ContactNameInput};
use astra_viewmodel::{
    CreateTargetRow, EditableCardRow, Monogram, PeopleSnapshot, PeopleWriteRow, PeopleWriteState,
    PersonDetail, PersonRow, PersonSourceRow, PersonValueKind, PersonValueRow,
};
use engine_api::{AccountId, AddressBookId, ContactId, PersonId, RecipientSuggestion};

use super::{people_avatar_of, people_card_of, people_of, people_suggestions_of, SUGGESTION_LIMIT};
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The fixture account the people commands name.
fn account() -> AccountId {
    AccountId::try_from("work").expect("an account id")
}

/// The Ada card the seed tests ask for, as the reference names it.
fn card() -> astra_app::CardRef {
    astra_app::CardRef {
        person: PersonId::new(7).unwrap(),
        account: account(),
        card: ContactId::try_from("/dav/card/work/default/ada.vcf").unwrap(),
    }
}

#[test]
fn the_people_pull_round_trips_the_published_snapshot() {
    let app = FakeApp::new();
    let snapshot = PeopleSnapshot {
        rows: vec![PersonRow {
            person: PersonId::new(7).unwrap(),
            display_name: "Ada Lovelace".to_owned(),
            monogram: Monogram {
                initials: "AL".to_owned(),
                colour: "#4DB6AC".to_owned(),
            },
            account_count: Some(2),
            section: "A".to_owned(),
            primary_address: Some("ada@example.test".to_owned()),
        }],
        detail: Some(PersonDetail {
            person: PersonId::new(7).unwrap(),
            display_name: "Ada Lovelace".to_owned(),
            sources: vec![PersonSourceRow {
                account: account(),
                card: ContactId::try_from("/dav/card/work/default/ada.vcf").unwrap(),
                writable: true,
            }],
            values: vec![PersonValueRow {
                kind: PersonValueKind::Email,
                value: "ada@example.test".to_owned(),
                sources: Vec::new(),
            }],
            editable_cards: vec![EditableCardRow {
                person: PersonId::new(7).unwrap(),
                account: account(),
                card: ContactId::try_from("/dav/card/work/default/ada.vcf").unwrap(),
            }],
        }),
        write_states: vec![PeopleWriteRow {
            account: account(),
            state: PeopleWriteState::Writing,
        }],
        create_targets: vec![CreateTargetRow {
            account: account(),
            book: AddressBookId::try_from("/dav/card/work/default/").unwrap(),
            name: "Contacts".to_owned(),
            is_default: true,
        }],
    };
    *app.people.lock().unwrap() = snapshot.clone();
    let state = state("people-pull", Arc::clone(&app), FakeStore::accepting());

    assert_eq!(people_of(&state), snapshot, "the pull answers whole");
}

#[test]
fn the_card_seed_threads_the_reference_and_answers_the_editor_s_fields() {
    let app = FakeApp::new();
    *app.card_seed.lock().unwrap() = Some(ContactDraftInput {
        name: Some(ContactNameInput {
            given: "Ada".to_owned(),
            surname: Some("Lovelace".to_owned()),
        }),
        organisation: Some("Ministry of Compute".to_owned()),
        role: None,
        emails: vec!["ada@example.test".to_owned()],
        phones: Vec::new(),
    });
    let state = state("people-card", Arc::clone(&app), FakeStore::accepting());

    let seed = state
        .runtime
        .block_on(|| people_card_of(&state, card()))
        .expect("the stored card seeds the editor");
    assert_eq!(seed.name.as_ref().unwrap().given, "Ada");
    assert_eq!(
        app.seeded_into.lock().unwrap().as_slice(),
        [(
            7_u64,
            "work".to_owned(),
            "/dav/card/work/default/ada.vcf".to_owned(),
        )],
        "the reference crossed to the core in its three parts",
    );
}

#[test]
fn an_unstored_card_seeds_nothing() {
    let app = FakeApp::new();
    let state = state(
        "people-card-absent",
        Arc::clone(&app),
        FakeStore::accepting(),
    );

    assert!(
        state
            .runtime
            .block_on(|| people_card_of(&state, card()))
            .is_none(),
        "a card nobody stores seeds nothing"
    );
}

#[test]
fn the_suggestions_pull_hands_the_token_down_with_the_limit() {
    let app = FakeApp::new();
    *app.suggestions.lock().unwrap() = vec![RecipientSuggestion {
        email: engine_api::CanonicalEmail::parse("ada@example.test").unwrap(),
        person_id: Some(PersonId::new(7).unwrap()),
        display_name: "Ada Lovelace".to_owned(),
        provenance: BTreeSet::default(),
        sent_count: 3,
        last_sent: None,
        is_saved: true,
    }];
    let state = state("people-suggest", Arc::clone(&app), FakeStore::accepting());

    let suggestions = state
        .runtime
        .block_on(|| people_suggestions_of(&state, "ad"));
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].display_name, "Ada Lovelace");
    assert_eq!(
        app.suggested_into.lock().unwrap().as_slice(),
        [("ad".to_owned(), SUGGESTION_LIMIT)],
        "the token crossed verbatim with the pull's own limit",
    );
}

#[test]
fn the_avatar_pull_threads_the_address_and_answers_the_core_s_view() {
    let app = FakeApp::new();
    *app.avatar.lock().unwrap() = AvatarView {
        initials: "AL".to_owned(),
        colour: "#4DB6AC".to_owned(),
        photo: Some(r"C:\blobs\ada.png".to_owned()),
    };
    let state = state("people-avatar", Arc::clone(&app), FakeStore::accepting());

    let view = state
        .runtime
        .block_on(|| people_avatar_of(&state, "ada@example.test"));
    assert_eq!(view.initials, "AL");
    assert_eq!(view.colour, "#4DB6AC");
    assert_eq!(
        view.photo.as_deref(),
        Some(r"C:\blobs\ada.png"),
        "the cached file's path crosses whole",
    );
    assert_eq!(
        app.avatar_into.lock().unwrap().as_slice(),
        ["ada@example.test".to_owned()],
        "the address crossed to the core verbatim",
    );
}

#[test]
fn the_base64_coder_matches_the_rfc_vectors() {
    assert_eq!(super::base64_standard(b""), "");
    assert_eq!(super::base64_standard(b"f"), "Zg==");
    assert_eq!(super::base64_standard(b"fo"), "Zm8=");
    assert_eq!(super::base64_standard(b"foo"), "Zm9v");
    assert_eq!(super::base64_standard(b"foob"), "Zm9vYg==");
    assert_eq!(super::base64_standard(b"fooba"), "Zm9vYmE=");
    assert_eq!(super::base64_standard(b"foobar"), "Zm9vYmFy");
}

#[test]
fn a_photo_inside_the_root_serves_and_a_traveller_refuses() {
    let dir = crate::commands_fakes::scratch("people-photo");
    let root = dir.join("photos");
    std::fs::create_dir_all(&root).expect("the photo root");
    let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
    std::fs::write(root.join("ada.blob"), png).expect("the planted photo");

    let served =
        super::photo_data_uri(&root, &root.join("ada.blob")).expect("the planted png serves");
    let expected = format!("data:image/png;base64,{}", super::base64_standard(&png));
    assert_eq!(served, expected, "the bytes round through whole");

    // A path outside the root, however it is spelled, answers nothing.
    let outside = dir.join("secret.txt");
    std::fs::write(&outside, b"no bytes of yours").expect("the outsider");
    assert!(
        super::photo_data_uri(&root, &outside).is_none(),
        "a file outside the photo area never serves"
    );
    assert!(
        super::photo_data_uri(&root, &dir.join("photos\\..\\secret.txt")).is_none(),
        "a traversal spelling never serves"
    );

    // Not an image: refused rather than served as the wrong type.
    std::fs::write(root.join("note.blob"), b"plain text").expect("the non-image");
    assert!(super::photo_data_uri(&root, &root.join("note.blob")).is_none());
}
