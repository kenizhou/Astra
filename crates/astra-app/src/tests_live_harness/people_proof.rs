// SPDX-License-Identifier: GPL-3.0-only
//! The shared fixtures of the gated people proofs: the dav-section config,
//! the boot-shaped two-account suite (the mail dial, the appended calendar
//! leg, the installed contacts leg, one engine store), the vCard builder,
//! the raw CardDAV seeding helpers, and the snapshot reader the proofs
//! assert through. Seeding speaks CardDAV directly through the shared raw
//! request helper (calendar_proof); every assertion goes through the
//! product's own engine and snapshot paths, never the raw helper. The
//! proofs themselves live in `people`, the same split the calendar proofs
//! made for the 500-line cap.

use std::{fmt::Write as _, sync::Arc};

use astra_account::{connect_dav_providers, DavAccount, Secret};
use astra_viewmodel::{PeopleSnapshot, PersonRow};
use engine_api::{AccountId, Engine, Provider};

use crate::{Account, App, AppObserver};

use super::calendar_proof::{dav_raw, encode_path_segment};
use super::proof;
use super::CountingObserver;

/// The harness account config carrying the dav section the boot loads: the
/// same imap and smtp legs the mail dial walks, plus the section naming both
/// dav origins on the shared plain-HTTP listener, basic credentials, so the
/// account ids the section's dial installs under match the mail dial's.
pub(super) fn dav_config(
    host: &str,
    port: &str,
    login: &str,
    password: &str,
) -> astra_account::AccountConfig {
    let base = super::jmap_base().expect("the gated run names the harness origin");
    let mut config = super::account(host, port, login, password, Vec::new());
    config.dav = Some(DavAccount {
        caldav_url: Some(base.clone()),
        carddav_url: Some(base),
        username: login.to_owned(),
        password: Some(Secret::new(password.to_owned())),
    });
    config
}

/// The boot-shaped people suite over one shared engine store: for each
/// harness account the mail dial (the certificate acceptance walk) with the
/// CalDAV provider the real `connect_dav_providers` returned appended after
/// the mail providers, and the CardDAV leg installed beside the account once
/// the app exists, the assembly `boot.rs` performs. Returns the app with
/// astra1's and astra2's account ids.
pub(super) async fn people_suite(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (App<Box<dyn Provider>>, AccountId, AccountId) {
    let engine = Arc::new(Engine::open_in_memory().expect("the engine store"));
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let mut accounts = Vec::new();
    let mut legs = Vec::new();
    let mut ids = Vec::new();
    for login in [user.to_owned(), proof::RECIPIENT.to_owned()] {
        let (account_id, mut providers, ..) =
            super::dial_with_accepted_certificate(host, port, &login, password).await;
        let dav = connect_dav_providers(&dav_config(host, port, &login, password))
            .await
            .expect("the dav dial");
        providers.extend(dav.caldav);
        accounts.push(Account {
            id: account_id.clone(),
            providers,
            identity: engine_api::EmailAddress::new(&login),
        });
        legs.push((account_id.clone(), dav.carddav));
        ids.push(account_id);
    }
    let app = App::new(engine, accounts, None, observer);
    for (account, leg) in legs {
        if let Some(leg) = leg {
            app.install_contacts(&account, leg);
        }
    }
    let mut ids = ids.into_iter();
    (
        app,
        ids.next().expect("astra1's id"),
        ids.next().expect("astra2's id"),
    )
}

/// One seeded card's vCard bytes: the `UID` every seeding mints, the
/// formatted name when the card carries one, and the extra lines the caller
/// names (emails, organisations, titles).
pub(super) fn card_vcf(uid: &str, name: Option<&str>, extra: &[&str]) -> String {
    let mut out = String::from("BEGIN:VCARD\r\nVERSION:4.0\r\n");
    let _ = write!(out, "UID:{uid}\r\n");
    if let Some(name) = name {
        let _ = write!(out, "FN:{name}\r\n");
    }
    for line in extra {
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str("END:VCARD\r\n");
    out
}

/// The card face's href for one uid under `account`'s default book.
pub(super) fn card_href(account: &str, uid: &str) -> String {
    format!(
        "/dav/card/{account}/default/{}.vcf",
        encode_path_segment(uid)
    )
}

/// Seeds one card onto `account`'s default book through the raw CardDAV
/// face: seeding only, never an assertion path.
pub(super) fn seed_card(account: &str, uid: &str, password: &str, body: &str) -> u16 {
    dav_raw(
        "PUT",
        &card_href(account, uid),
        account,
        password,
        "text/vcard",
        body,
    )
}

/// Withdraws the card at `href` through the raw CardDAV face: seeding-side
/// cleanup for a product that ships no contact delete, never an assertion
/// path. `href` is the provider's own id (the full path) for cards the
/// product created; `login` is the account's own login, never its engine id.
pub(super) fn withdraw_card(login: &str, href: &str, password: &str) -> u16 {
    dav_raw("DELETE", href, login, password, "text/vcard", "")
}

/// The snapshot's rows whose identity carries `needle` (the display name, or
/// the primary address when the sources carry no name): the proofs' read
/// fixture over the product's own snapshot.
pub(super) fn rows_carrying<'a>(snapshot: &'a PeopleSnapshot, needle: &str) -> Vec<&'a PersonRow> {
    snapshot
        .rows
        .iter()
        .filter(|row| {
            row.display_name.contains(needle)
                || row
                    .primary_address
                    .as_deref()
                    .is_some_and(|address| address.contains(needle))
        })
        .collect()
}
