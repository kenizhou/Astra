// SPDX-License-Identifier: GPL-3.0-only
//! The gated proof of the sender avatar against the live harness: a seeded
//! card carrying an inline photo whose address matches the sender of a note
//! the harness seed pins, read back through the product's own sync so the
//! note's row resolves its avatar through the real cache. The monogram
//! stands while the cache holds nothing (the hot-path read is store work
//! only), one fetch through the installed CardDAV leg fills the cache, and
//! the cached photo's file answers after. The card is withdrawn through the
//! raw seeding helper at the end (the product ships no contact delete), and
//! the note itself is the harness's own seed, so nothing else moves.

use crate::{FolderRef, Intent, PeopleIntent};

use super::calendar_proof::run_token;
use super::people_proof::{
    card_href, card_vcf, people_suite, rows_carrying, seed_card, withdraw_card,
};
use super::proof;
use super::{harness, inbox_key, row_by_subject};

/// A one pixel PNG, base64: the photo the seeded card carries inline, small
/// enough to seed in one vCard line and real enough that the engine's cache
/// stores bytes a host's sniffing command could type.
const PHOTO_PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// The sender's row resolves its avatar through the real cache: the note the
/// harness seed delivers into astra1's INBOX names astra2 as its sender, the
/// seeded card carrying astra2's address and a photo joins into the person
/// the row's ask reads, and the avatar crosses monogram first, photo after
/// the one fetch that fills the engine's cache.
#[tokio::test]
async fn the_sender_s_row_resolves_its_avatar_through_the_real_cache() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, first, _second) = people_suite(&host, &port, &user, &password).await;
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    // The door guard, before this run seeds anything: the proof's surname is
    // minted by every run of it, so a card left behind by a panicked prior
    // run fails here rather than passing silently beneath this run's token.
    assert!(
        rows_carrying(&app.people(), "Photon").is_empty(),
        "no avatar-proof card survives a prior run"
    );

    let token = run_token();
    // The token rides inside the name (the last word stays "Photon", so the
    // monogram's initials are stable whatever the token mints).
    let name = format!("Ada {token} Photon");
    let uid = format!("photo-{token}@astra.local");
    let card = card_vcf(
        &uid,
        Some(&name),
        &[
            &format!("EMAIL:{}", proof::RECIPIENT),
            &format!("PHOTO;ENCODING=b;TYPE=PNG:{PHOTO_PNG}"),
        ],
    );
    assert_eq!(
        seed_card(&user, &uid, &password, &card),
        201,
        "the seeding PUT stores the card on the reader's own book"
    );

    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    let people = app.people();
    assert_eq!(
        rows_carrying(&people, &token).len(),
        1,
        "the seeded card renders as one person through the product's sync"
    );

    // The note the harness seed pins, its row read through the product's own
    // list: the sender is the address the card carries.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(first.as_str(), inbox_key(&app, &first))
        .expect("the reader's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let list = app.mailbox_list();
    let row = row_by_subject(&list, "Harness note one");
    assert_eq!(
        row.sender_address,
        proof::RECIPIENT,
        "the seeded note's sender is the address the card carries"
    );

    // The monogram until the photo lands: the cache holds nothing yet, and
    // the hot-path read does store work only, so the row's ask draws the
    // person's initials and no photo.
    let before = app.avatar_by_email(&row.sender_address).await;
    assert_eq!(before.initials, "AP", "the joined person's monogram");
    assert_eq!(
        before.photo, None,
        "nothing has filled the cache, so no photo crosses"
    );

    // The fill, the background pass the engine's own doc describes: one
    // fetch through the installed CardDAV leg (the provider decodes the
    // card's inline data URI) stores the cache the read answers from.
    app.dispatch(Intent::People(PeopleIntent::OpenPerson {
        person: rows_carrying(&app.people(), &token)[0].person,
    }))
    .await
    .unwrap();
    let detail = app.people().detail.expect("the open publishes the detail");
    let source = &detail.sources[0];
    assert_eq!(
        &source.account, &first,
        "the card stands on the reader's account"
    );
    let card = app
        .engine
        .contact_card(&source.account, &source.card)
        .await
        .expect("the stored card read")
        .expect("the synced card is stored");
    let media = card
        .media
        .values()
        .map(|property| &property.value)
        .find(|resource| resource.kind.as_deref() == Some("photo"))
        .expect("the card carries a photo resource");
    let leg = app
        .people_legs(&source.account)
        .first()
        .expect("the account's installed contacts leg")
        .clone();
    let fetched = app
        .engine
        .contact_photo(leg.as_ref(), &source.account, &card, media)
        .await
        .expect("the fetch through the real leg")
        .expect("the inline photo fills the cache");
    assert!(fetched.path.exists(), "the cached file stands on disk");

    // The photo after: the same ask now answers with the cached file's path,
    // the monogram's initials still riding beside it.
    let after = app.avatar_by_email(&row.sender_address).await;
    assert_eq!(after.initials, "AP", "the monogram still derives the same");
    let photo = after.photo.expect("the cached photo answers the read");
    assert!(
        std::path::Path::new(&photo).exists(),
        "the photo path names a real file"
    );
    assert_eq!(photo, fetched.path.display().to_string());

    // The withdrawal: the card leaves through the raw seeding helper, and
    // the person it joined renders nowhere after one refresh. The note is
    // the harness's own seed and stays.
    assert_eq!(
        withdraw_card(&user, &card_href(&user, &uid), &password),
        204,
        "the seeding-side cleanup withdraws the card"
    );
    app.dispatch(Intent::People(PeopleIntent::Refresh))
        .await
        .unwrap();
    assert!(
        rows_carrying(&app.people(), &token).is_empty(),
        "the withdrawn card's person renders nowhere after one refresh"
    );
}
