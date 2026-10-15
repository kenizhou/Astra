// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the host's account store: the settings seam over the dial
//! table and the credential store. The read half answers one masked row
//! per registered config; the write halves pin the merge rule (a stated
//! field replaces what stood, an absent one keeps it, the password
//! keeping whenever the editor states none), the dial ahead of the
//! persist (a failed re-dial leaves the stored section exactly as it
//! stood), and the removal's two halves. The dials are injected, so no
//! test touches a network; the real dial is the boot's own connect.

use std::sync::Arc;

use async_trait::async_trait;
use engine_api::{
    AccountId, CalendarWrites, Capabilities, ContactsProvider, MailboxWrites, Provider,
};
use engine_provider::ConnectionInfo;

use astra_app::{DavSectionInput, HostAccountOps};

use super::HostAccountStore;
use crate::commands_fakes::{scratch, FakeStore};
use crate::credential_store::AccountCredentialStore;
use crate::dial_table::AccountDials;

/// A provider stand-in for a dialled leg: no served data, only the
/// connection it would advertise, the same shape the boot's own tests
/// use.
struct FakeDav {
    calendars: bool,
}

#[async_trait]
impl Provider for FakeDav {
    fn connection_info(&self) -> ConnectionInfo {
        let mut caps = Capabilities::none().with_mail();
        if self.calendars {
            caps = caps.with_calendars();
        }
        ConnectionInfo::new(caps)
    }
}

impl MailboxWrites for FakeDav {}
impl CalendarWrites for FakeDav {}

#[async_trait]
impl ContactsProvider for FakeDav {}

/// A stored config that pins its id and carries a full dav section.
fn dav_config_toml() -> &'static str {
    concat!(
        "id = \"work\"\n",
        "[imap]\n",
        "addr = \"imap.example.net:993\"\n",
        "server_name = \"imap.example.net\"\n",
        "username = \"ada@example.test\"\n",
        "password = \"mail-secret\"\n",
        "[dav]\n",
        "caldav_url = \"https://dav.example.test\"\n",
        "carddav_url = \"https://cards.example.test\"\n",
        "username = \"ada@example.test\"\n",
        "password = \"dav-secret\"\n",
    )
}

/// A mail-only config, pinned the same way.
fn mail_config_toml() -> &'static str {
    concat!(
        "id = \"home\"\n",
        "[imap]\n",
        "addr = \"imap.example.net:993\"\n",
        "server_name = \"imap.example.net\"\n",
        "username = \"grace@example.test\"\n",
        "password = \"mail-secret\"\n",
    )
}

/// The work account's id, the way its config pins it.
fn work() -> AccountId {
    AccountId::try_from("work").expect("an account id")
}

/// A scratch dial table holding both fixture accounts.
fn table(name: &str) -> Arc<AccountDials> {
    let dials = Arc::new(AccountDials::empty(&scratch(name)));
    for toml in [dav_config_toml(), mail_config_toml()] {
        let config = astra_account::load_str(toml).expect("the fixture config loads");
        let id = config.account_id().expect("the fixture pins its id");
        dials
            .register(&id, config, astra_account::ImapConnections::new())
            .expect("the scratch table registers");
    }
    dials
}

/// The store under test over `dials` and a persisting fake credential
/// store that already holds the fixtures' documents, handed back beside
/// it so the document asserts read what it holds.
fn store(dials: &Arc<AccountDials>) -> (HostAccountStore, Arc<FakeStore>) {
    let credentials = FakeStore::accepting();
    for (id, toml) in [("work", dav_config_toml()), ("home", mail_config_toml())] {
        credentials
            .persist(id, toml)
            .expect("the seed document persists");
    }
    (ops_over(dials, &credentials), credentials)
}

/// The store under test over `dials` and `credentials`: the explicit
/// cast, because a nested `Arc::clone` argument does not unsize on its
/// own.
fn ops_over(dials: &Arc<AccountDials>, credentials: &Arc<FakeStore>) -> HostAccountStore {
    let credentials = Arc::clone(credentials) as Arc<dyn AccountCredentialStore>;
    HostAccountStore::new(
        Arc::clone(dials),
        credentials,
        Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
    )
}

/// The newest document the credential store holds for `id`.
fn newest(credentials: &FakeStore, id: &str) -> String {
    let entries = credentials.entries.lock().unwrap();
    entries
        .iter()
        .rev()
        .find(|(held, _)| held == id)
        .map(|(_, toml)| toml.clone())
        .unwrap_or_default()
}

/// The dav section the dial table holds for `id`.
fn standing_dav(dials: &AccountDials, id: &str) -> astra_account::DavAccount {
    let (config, _) = dials.of(id).expect("the table still holds the account");
    config.dav.expect("the section stands")
}

#[test]
fn account_infos_answers_one_masked_row_per_registered_config() {
    let dials = table("infos");
    let (ops, _credentials) = store(&dials);
    let infos = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| ops.account_infos());

    assert_eq!(infos.len(), 2, "one row per registered config");
    let home = infos
        .iter()
        .find(|info| info.id.as_str() == "home")
        .expect("the home row stands");
    assert_eq!(
        home.kind,
        astra_viewmodel::AccountKind::ImapSmtp,
        "every stored config names an imap section"
    );
    assert!(home.dav.is_none(), "a mail-only config carries no dav echo");
    let row = infos
        .iter()
        .find(|info| info.id.as_str() == "work")
        .expect("the work row stands")
        .dav
        .as_ref()
        .expect("a dav-carrying config echoes its section");
    assert_eq!(
        row.caldav_url.as_deref(),
        Some("https://dav.example.test"),
        "the CalDAV origin crosses"
    );
    assert_eq!(
        row.carddav_url.as_deref(),
        Some("https://cards.example.test"),
        "the CardDAV origin crosses"
    );
    assert_eq!(row.username, "ada@example.test");
    assert!(
        row.dav_password_set,
        "the echo answers that a credential stands, never the credential itself"
    );
}

#[test]
fn a_stated_field_replaces_and_an_absent_one_keeps() {
    let dials = table("merge");
    let (ops, credentials) = store(&dials);
    let account = work();
    let legs = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| {
            ops.store_dav_with(
                &account,
                Some(DavSectionInput {
                    caldav_url: None,
                    carddav_url: Some("   ".to_owned()),
                    username: Some("ada@elsewhere.test".to_owned()),
                    password: None,
                }),
                |config| async move {
                    assert!(
                        config
                            .dav
                            .as_ref()
                            .is_some_and(|dav| dav.carddav_url.is_none()),
                        "the dial sees the merged config, its CardDAV origin cleared"
                    );
                    Ok::<_, astra_account::AccountError>(astra_account::DavProviders {
                        caldav: Some(Box::new(FakeDav { calendars: true })),
                        carddav: None,
                    })
                },
            )
        })
        .expect("the save dials and stores");

    assert!(
        legs.caldav
            .as_ref()
            .is_some_and(|leg| leg.connection_info().capabilities.calendars()),
        "the answered calendar leg is the dialled provider"
    );
    assert!(legs.carddav.is_none(), "no contacts leg without an origin");

    let stored =
        astra_account::load_str(&newest(&credentials, "work")).expect("the merged document loads");
    let dav = stored.dav.as_ref().expect("the section stands");
    assert_eq!(
        dav.caldav_url.as_deref(),
        Some("https://dav.example.test"),
        "an absent field keeps what stood"
    );
    assert!(
        dav.carddav_url.is_none(),
        "a blank stated field clears the origin"
    );
    assert_eq!(dav.username, "ada@elsewhere.test");
    assert!(
        dav.password
            .as_ref()
            .is_some_and(|secret| secret.expose() == "dav-secret"),
        "an absent password keeps the stored credential"
    );
    assert_eq!(
        standing_dav(&dials, "work").username,
        "ada@elsewhere.test",
        "the dial table carries the merged config"
    );
}

#[test]
fn a_stated_password_replaces_the_stored_one() {
    let dials = table("password");
    let (ops, credentials) = store(&dials);
    let account = work();
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| {
            ops.store_dav_with(
                &account,
                Some(DavSectionInput {
                    password: Some("app-password-2".to_owned()),
                    ..DavSectionInput::default()
                }),
                |_config| async {
                    Ok::<_, astra_account::AccountError>(astra_account::DavProviders {
                        caldav: None,
                        carddav: None,
                    })
                },
            )
        })
        .expect("the save dials and stores");

    let stored =
        astra_account::load_str(&newest(&credentials, "work")).expect("the merged document loads");
    assert_eq!(
        stored
            .dav
            .and_then(|dav| dav.password.map(|secret| secret.expose().to_owned())),
        Some("app-password-2".to_owned()),
        "a stated password replaces the stored credential"
    );
}

#[test]
fn a_failed_re_dial_answers_err_and_the_stored_config_still_stands() {
    let dials = table("failed-dial");
    let credentials = FakeStore::accepting();
    credentials
        .persist("work", dav_config_toml())
        .expect("the seed document persists");
    let ops = ops_over(&dials, &credentials);
    let account = work();

    let sentence = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| {
            ops.store_dav_with(
                &account,
                Some(DavSectionInput {
                    username: Some("ada@elsewhere.test".to_owned()),
                    ..DavSectionInput::default()
                }),
                |_config| async {
                    Err::<astra_account::DavProviders, _>(astra_account::AccountError::Dav(
                        "the origin is unreachable".to_owned(),
                    ))
                },
            )
        })
        .expect_err("the failed dial refuses the save");

    assert!(
        sentence.contains("unreachable"),
        "the dial's own sentence crosses: {sentence}"
    );
    assert_eq!(
        credentials.entries.lock().unwrap().len(),
        1,
        "the failed re-dial persists nothing host-side"
    );
    let standing = standing_dav(&dials, "work");
    assert_eq!(
        standing.username, "ada@example.test",
        "the stored section stands"
    );
    assert!(
        standing
            .password
            .as_ref()
            .is_some_and(|secret| secret.expose() == "dav-secret"),
        "the stored credential stands"
    );
}

#[test]
fn a_persist_refusal_answers_err_and_nothing_stored_moves() {
    let dials = table("persist-refused");
    let ops = HostAccountStore::new(
        Arc::clone(&dials),
        FakeStore::refusing(),
        Arc::new(crate::oauth::grant_refusals::GrantRefusals::new()),
    );
    let account = work();

    let sentence = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| {
            ops.store_dav_with(
                &account,
                Some(DavSectionInput {
                    username: Some("ada@elsewhere.test".to_owned()),
                    ..DavSectionInput::default()
                }),
                |_config| async {
                    Ok::<_, astra_account::AccountError>(astra_account::DavProviders {
                        caldav: None,
                        carddav: None,
                    })
                },
            )
        })
        .expect_err("the refused persist refuses the save");

    assert!(
        sentence.contains("refused the write"),
        "the store's own sentence crosses: {sentence}"
    );
    assert_eq!(
        standing_dav(&dials, "work").username,
        "ada@example.test",
        "the dial table keeps the section that stood"
    );
}

#[test]
fn a_merged_section_naming_no_origin_is_refused_before_anything_moves() {
    let dials = table("no-origin");
    let (ops, credentials) = store(&dials);

    // Over the mail-only account, no base stands to keep.
    let home = AccountId::try_from("home").expect("an account id");
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let sentence = runtime
        .block_on(|| {
            ops.store_dav_with(
                &home,
                Some(DavSectionInput {
                    username: Some("grace@example.test".to_owned()),
                    ..DavSectionInput::default()
                }),
                |_config| async {
                    panic!("a section naming no origin never reaches a dial");
                },
            )
        })
        .expect_err("the section is refused");
    assert_eq!(
        sentence, "the merged section names no origin to dial",
        "the refusal is the seam's own sentence"
    );

    // Blank-stated origins over a carrying section clear both.
    let account = work();
    let sentence = runtime
        .block_on(|| {
            ops.store_dav_with(
                &account,
                Some(DavSectionInput {
                    caldav_url: Some("  ".to_owned()),
                    carddav_url: Some(String::new()),
                    ..DavSectionInput::default()
                }),
                |_config| async {
                    panic!("a section naming no origin never reaches a dial");
                },
            )
        })
        .expect_err("the cleared section is refused");
    assert_eq!(sentence, "the merged section names no origin to dial");

    assert_eq!(
        standing_dav(&dials, "work").username,
        "ada@example.test",
        "nothing moved"
    );
    assert_eq!(
        credentials.entries.lock().unwrap().len(),
        2,
        "nothing persisted"
    );
}

#[test]
fn a_none_section_clears_the_legs_and_the_stored_section() {
    let dials = table("clear");
    let (ops, credentials) = store(&dials);
    let account = work();
    let legs = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| {
            ops.store_dav_with(&account, None, |config| async move {
                assert!(
                    config.dav.is_none(),
                    "the dial sees the config with its section dropped"
                );
                Ok::<_, astra_account::AccountError>(astra_account::DavProviders {
                    caldav: None,
                    carddav: None,
                })
            })
        })
        .expect("the clear dials and stores");

    assert!(
        legs.caldav.is_none() && legs.carddav.is_none(),
        "the clear answers empty legs"
    );
    let stored =
        astra_account::load_str(&newest(&credentials, "work")).expect("the cleared document loads");
    assert!(
        stored.dav.is_none(),
        "the persisted document drops the section"
    );
    let (config, _) = dials.of("work").expect("the table still holds the account");
    assert!(config.dav.is_none(), "the dial table drops the section");
}

#[test]
fn an_account_the_table_does_not_hold_is_refused() {
    let dials = table("unknown");
    let (ops, _credentials) = store(&dials);
    let ghost = AccountId::try_from("ghost").expect("an account id");

    let sentence = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(|| {
            ops.store_dav_with(&ghost, Some(DavSectionInput::default()), |_config| async {
                panic!("an unregistered account never reaches a dial");
            })
        })
        .expect_err("the unknown account is refused");
    assert_eq!(
        sentence, "the account has no stored config on this host",
        "the refusal names what is missing"
    );
}

// The graph-refusal half, split into its own file the same way: the info
// rows a stored grant answers (the Microsoft kind, the address, the
// standing refusal state) and the removal's refusal bookkeeping.
#[path = "account_ops_graph_tests.rs"]
mod graph;

// The removal half, split into its own file so this one stays under the
// line cap.
#[path = "account_ops_removal_tests.rs"]
mod removal;
