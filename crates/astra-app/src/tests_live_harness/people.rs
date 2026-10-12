// SPDX-License-Identifier: GPL-3.0-only
//! The gated people proofs against the live harness: the dav dial (the real
//! `connect_dav_providers` over a dav section naming both origins, then the
//! boot-shaped install), the sync read (the seeded cards render through the
//! product's own snapshot with the fold order, the monogram, the section
//! letters, and the cross-account disclosure), the write round trip (a create
//! into the writable default book and a rename on the named card, read back
//! through the product's sync), and the autosuggest ranking over this run's
//! own sent history.
//!
//! The seeding discipline is the calendar proofs': cards are seeded straight
//! onto the harness's CardDAV face through the raw helper under unique run
//! tokens, every assertion reads through the product's own engine paths and
//! snapshots, and the run withdraws what it seeded. The one asymmetry is
//! honest: the product ships no contact delete, so a run withdraws its cards
//! through the same raw seeding helper it seeded them with (seeding-side
//! cleanup, never an assertion path), exactly as the mail suites withdraw
//! their copies through product paths where a product path exists. The
//! shared fixtures live in `people_proof`.

use std::time::Duration;

use astra_account::connect_dav_providers;
use astra_viewmodel::PersonValueKind;
use engine_api::{AccountId, Draft, EmailAddress};

use crate::protocol::{ContactDraftInput, ContactNameInput, ContactPatchInput};
use crate::reference::{AddressBookRef, CardRef};
use crate::{CalendarIntent, Intent, PeopleIntent};

use super::calendar_proof::{range, run_token, DELIVERY_TIMEOUT};
use super::harness;
use super::people_proof::{
    card_href, card_vcf, dav_config, people_suite, rows_carrying, seed_card, withdraw_card,
};
use super::proof;

/// The dav dial the desktop boot performs, live: the real
/// `connect_dav_providers` over a stored config's dav section naming both
/// origins on the harness's shared listener connects BOTH legs, and the
/// boot-shaped assembly serves them: the appended calendar provider fills
/// the calendar surface, and the installed contacts leg advertises the
/// writable book through the people snapshot's create targets. This closes
/// the live dial the account extension deferred to this proof.
#[tokio::test]
async fn the_dav_dial_connects_both_legs_and_the_boot_shape_installs_them() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let legs = connect_dav_providers(&dav_config(&host, &port, &user, &password))
        .await
        .expect("both dav legs dial the harness origin");
    assert!(legs.caldav.is_some(), "the calendar leg connected");
    assert!(legs.carddav.is_some(), "the contacts leg connected");

    let (app, first, second) = people_suite(&host, &port, &user, &password).await;
    app.dispatch(range()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    let snapshot = app.calendar();
    for account in [&first, &second] {
        assert!(
            snapshot
                .calendars
                .iter()
                .any(|calendar| &calendar.account == account),
            "the appended calendar provider fills the account's calendars"
        );
    }
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let snapshot = app.people();
    for account in [&first, &second] {
        assert!(
            snapshot
                .create_targets
                .iter()
                .any(|target| &target.account == account),
            "the installed contacts leg advertises the account's writable book (the harness listing carries no default marker)"
        );
    }
}

/// A people sync over the running server renders the seeded cards through
/// the product's own snapshot: the fold lays the alphabet out first with the
/// accented letter filed under its base, the digit-led nameless address
/// lands in the `#` bucket, the monogram keeps the real characters, and the
/// cross-account pair joins into ONE row whose disclosure counts two
/// accounts and whose detail names both sources.
#[tokio::test]
async fn the_people_sync_renders_the_seeded_cards_with_the_fold_and_the_disclosure() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let anna = "Anna Arctic";
    let emile = "Émile Zed";
    let duo = "Duo Delta";
    let bare = format!("9bare-{token}@example.test");
    let shared = format!("duo-{token}@example.test");
    let seeds = [
        (
            user.as_str(),
            format!("anna-{token}@astra.local"),
            card_vcf(
                &format!("anna-{token}@astra.local"),
                Some(anna),
                &[&format!("EMAIL:anna-{token}@example.test")],
            ),
        ),
        (
            user.as_str(),
            format!("emile-{token}@astra.local"),
            card_vcf(
                &format!("emile-{token}@astra.local"),
                Some(emile),
                &[&format!("EMAIL:emile-{token}@example.test")],
            ),
        ),
        (
            user.as_str(),
            format!("bare-{token}@astra.local"),
            card_vcf(
                &format!("bare-{token}@astra.local"),
                None,
                &[&format!("EMAIL:{bare}")],
            ),
        ),
        (
            user.as_str(),
            format!("duo-one-{token}@astra.local"),
            card_vcf(
                &format!("duo-one-{token}@astra.local"),
                Some(duo),
                &[&format!("EMAIL:{shared}")],
            ),
        ),
        (
            proof::RECIPIENT,
            format!("duo-two-{token}@astra.local"),
            card_vcf(
                &format!("duo-two-{token}@astra.local"),
                Some(duo),
                &[&format!("EMAIL:{shared}")],
            ),
        ),
    ];
    for (account, uid, body) in &seeds {
        assert_eq!(
            seed_card(account, uid, &password, body),
            201,
            "the seeding PUT stores the card"
        );
    }

    let (app, first, second) = people_suite(&host, &port, &user, &password).await;
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();

    let snapshot = app.people();
    let rows = rows_carrying(&snapshot, &token);
    assert_eq!(
        rows.len(),
        4,
        "the five seeded cards render as four people: the cross-account pair joins"
    );
    let names: Vec<&str> = rows.iter().map(|row| row.display_name.as_str()).collect();
    assert_eq!(
        names,
        [anna, duo, emile, ""],
        "the fold lays the alphabet out first, the # bucket last"
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.section.as_str())
            .collect::<Vec<_>>(),
        ["A", "D", "E", "#"],
        "the accented letter files under its base; the digit-led address lands in #"
    );
    assert_eq!(rows[0].monogram.initials, "AA", "Anna Arctic's initials");
    assert_eq!(
        rows[0].account_count, None,
        "one account crosses no In N accounts key"
    );
    assert_eq!(
        rows[2].monogram.initials, "ÉZ",
        "the monogram keeps the real character the name carries"
    );
    let duo_row = rows[1];
    assert_eq!(
        duo_row.account_count,
        Some(2),
        "the joined row says In 2 accounts"
    );
    assert_eq!(duo_row.primary_address.as_deref(), Some(shared.as_str()));
    assert_eq!(
        rows[3].display_name, "",
        "a card with no name renders an empty display name, the host's placeholder signal"
    );
    assert_eq!(
        rows[3].monogram.initials, "9",
        "the nameless card initials from its own address"
    );
    assert_eq!(rows[3].primary_address.as_deref(), Some(bare.as_str()));

    // The detail names both sources under Also in, and the shared address's
    // provenance names both cards.
    app.dispatch(Intent::People(PeopleIntent::OpenPerson {
        person: duo_row.person,
    }))
    .await
    .unwrap();
    let detail = app.people().detail.expect("the open publishes the detail");
    let mut accounts: Vec<&AccountId> = detail.sources.iter().map(|row| &row.account).collect();
    accounts.sort();
    let mut expected = vec![&first, &second];
    expected.sort();
    assert_eq!(accounts, expected, "the detail names both accounts' cards");
    assert_eq!(
        detail.editable_cards.len(),
        2,
        "both source cards are editable where they stand"
    );
    let shared_value = detail
        .values
        .iter()
        .find(|value| value.kind == PersonValueKind::Email && value.value == shared)
        .expect("the shared address rides the values");
    assert_eq!(
        shared_value.sources.len(),
        2,
        "the shared address's provenance names both cards"
    );

    // The withdrawal: raw, honestly, because the product ships no delete.
    for (account, uid, _) in &seeds {
        assert_eq!(
            withdraw_card(account, &card_href(account, uid), &password),
            204,
            "the seeding cleanup withdraws the card"
        );
    }
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    assert!(
        rows_carrying(&app.people(), &token).is_empty(),
        "the withdrawn cards render nowhere after one refresh"
    );
}

/// The contact write family round-tripped live: a create into the writable
/// default book lands on the server and reads back through the product's
/// sync, a rename on the named card lands the same way, and every write
/// state settles by the time its dispatch returns.
#[tokio::test]
async fn the_contact_round_trip_lands_every_write() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let (app, first, _second) = people_suite(&host, &port, &user, &password).await;
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let snapshot = app.people();
    let target = snapshot
        .create_targets
        .iter()
        .find(|target| target.account == first)
        .expect("the account's writable book (the harness listing carries no default marker)");
    let book = AddressBookRef::from_parts(first.as_str(), target.book.key().as_str().to_owned())
        .expect("the book reference");

    // The create: the engine-minted card lands on the server, read back
    // through the product's own sync.
    let created = format!("Cora Created {token}");
    app.dispatch(Intent::People(PeopleIntent::CreateContact {
        book,
        draft: ContactDraftInput {
            name: Some(ContactNameInput {
                given: created,
                surname: Some("Card".to_owned()),
            }),
            emails: vec![format!("cora-{token}@example.test")],
            ..ContactDraftInput::default()
        },
    }))
    .await
    .unwrap();
    assert!(
        app.people().write_states.is_empty(),
        "the create settled: no write row stands"
    );
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let snapshot = app.people();
    let rows = rows_carrying(&snapshot, &token);
    assert_eq!(rows.len(), 1, "the created person renders once");
    assert_eq!(rows[0].display_name, format!("Cora Created {token} Card"));
    let person = rows[0].person;

    // The edit: a rename on the card the detail names, never on the person
    // alone, landing through the same discipline.
    app.dispatch(Intent::People(PeopleIntent::OpenPerson { person }))
        .await
        .unwrap();
    let editable = app
        .people()
        .detail
        .expect("the open publishes the detail")
        .editable_cards;
    let editable = editable.first().expect("the created card is editable");
    app.dispatch(Intent::People(PeopleIntent::UpdateContact {
        card: CardRef {
            person: editable.person,
            account: editable.account.clone(),
            card: editable.card.clone(),
        },
        patch: ContactPatchInput {
            name: Some(ContactNameInput {
                given: format!("Cora Renamed {token}"),
                surname: Some("Card".to_owned()),
            }),
            ..ContactPatchInput::default()
        },
    }))
    .await
    .unwrap();
    assert!(
        app.people().write_states.is_empty(),
        "the edit settled: no write row stands"
    );
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let snapshot = app.people();
    let rows = rows_carrying(&snapshot, &token);
    assert_eq!(rows.len(), 1, "the edited person still renders once");
    assert_eq!(
        rows[0].display_name,
        format!("Cora Renamed {token} Card"),
        "the rename landed"
    );

    // The withdrawal: the product ships no contact delete, so the created
    // card is withdrawn through the raw seeding helper. The card id the
    // product reads back is the provider's own href (the full path), so the
    // raw DELETE needs only the account's login, never its engine id.
    app.dispatch(Intent::People(PeopleIntent::OpenPerson {
        person: rows[0].person,
    }))
    .await
    .unwrap();
    let source = &app
        .people()
        .detail
        .expect("the detail names the source card")
        .sources[0];
    assert_eq!(
        &source.account, &first,
        "the created card stands on its account"
    );
    assert_eq!(
        withdraw_card(&user, source.card.as_str(), &password),
        204,
        "the seeding-side cleanup withdraws the created card"
    );
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    assert!(
        rows_carrying(&app.people(), &token).is_empty(),
        "the withdrawn card renders nowhere after one refresh"
    );
}

/// Autosuggest ranks this run's own sent history: one fresh mail submitted
/// through the engine's send path, the sync passes that mine the recipient
/// history out of the sender's Sent mail, and the ranked read through the
/// product's own suggestion path. The store is a fresh in-memory engine, so
/// nothing ranked is residue; the mail is withdrawn afterwards through the
/// product's delete path so the harness keeps the shape the mail suites pin.
#[tokio::test]
async fn autosuggest_ranks_this_run_s_own_sent_history() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let subject = format!("Harness suggestion {token}");
    let (app, first, second) = people_suite(&host, &port, &user, &password).await;
    let handle = app
        .account_handle(&first)
        .await
        .expect("the sender's mail account");
    let provider = handle.providers.first().expect("the submission provider");
    let draft = Draft::new(
        crate::drafts::mint_message_id().expect("a fresh message id"),
        EmailAddress::new(&user),
        vec![EmailAddress::new(proof::RECIPIENT)],
        subject.clone(),
        "The mail the suggestion ranking reads.",
    );
    app.engine
        .submit_mail(provider, &first, &draft)
        .await
        .expect("the suggestion mail submits");

    // The bounded wait: the outbox drain files the Sent copy after the sync
    // pass that queued it, so the history this run minted ranks once a
    // later pass has mined it.
    let deadline = std::time::Instant::now() + DELIVERY_TIMEOUT;
    let top = loop {
        app.dispatch(Intent::RefreshMail).await.unwrap();
        let suggestions = app.recipient_suggestions("astra2", 10).await;
        if let Some(top) = suggestions
            .first()
            .filter(|top| top.email.as_str() == proof::RECIPIENT)
        {
            break top.clone();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the sent-to address never ranked first within the delivery timeout"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    assert!(top.sent_count >= 1, "the ranking sees this run's own send");
    assert!(
        !top.is_saved,
        "a history-only suggestion is ranked, never hidden"
    );

    // The bounded withdrawal: both copies through the product's delete
    // path, swept until neither stands. One fixed pass races the delivery
    // (a copy the sweep had not yet synced arrives with a later pass and
    // needs a delete of its own), and the mail suites pin astra2's INBOX
    // empty, so the run must leave nothing behind.
    let deadline = std::time::Instant::now() + DELIVERY_TIMEOUT;
    loop {
        app.dispatch(Intent::RefreshMail).await.unwrap();
        let mut residue = false;
        for account in [&first, &second] {
            residue |= app
                .engine
                .messages(account)
                .await
                .unwrap_or_default()
                .into_iter()
                .any(|message| message.envelope.subject.as_deref() == Some(subject.as_str()));
        }
        if !residue {
            break;
        }
        proof::delete_subject_mail(&app, &first, &[&subject]).await;
        proof::delete_subject_mail(&app, &second, &[&subject]).await;
        app.dispatch(Intent::RefreshMail).await.unwrap();
        assert!(
            std::time::Instant::now() < deadline,
            "the suggestion mail was not withdrawn within the delivery timeout"
        );
    }
}
