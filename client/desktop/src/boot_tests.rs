// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the boot's dav dial: the seam that folds a dialled dav pair
//! into an account (the calendar provider appended after the mail
//! providers, the contacts leg handed back for the install), and the rule
//! that a failed dav dial leaves an otherwise working account standing on
//! its mail half alone. The tests block on their own runtime (the host's
//! drive pattern): this crate forbids `#[tokio::test]`'s `block_on` by
//! clippy config.

use async_trait::async_trait;
use engine_api::{CalendarWrites, Capabilities, ContactsProvider, MailboxWrites, Provider};
use engine_provider::ConnectionInfo;

use super::{dial_dav_with, HostAccount, HostContactsLeg};
use crate::app_state::HostProvider;

/// A dial answer: what the fake dav dial hands back for one test.
type Dial =
    Box<dyn FnOnce() -> Result<astra_account::DavProviders, astra_account::AccountError> + Send>;

/// A provider stand-in for either dialled leg: no served data, only the
/// connection it would advertise.
struct FakeDav {
    calendars: bool,
}

impl FakeDav {
    /// The calendar leg's stand-in.
    fn calendar() -> Box<dyn Provider> {
        Box::new(Self { calendars: true })
    }

    /// The mail half's stand-in.
    fn mail() -> HostProvider {
        Box::new(Self { calendars: false })
    }
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

/// One account over a single mail provider, the shape every dialled account
/// has before the dav fold.
fn mail_account() -> HostAccount {
    HostAccount {
        id: engine_api::AccountId::try_from("work").unwrap(),
        providers: vec![FakeDav::mail()],
        identity: engine_api::EmailAddress::new("info@test.local"),
    }
}

/// A config naming both dav origins at one server.
fn dav_config() -> astra_account::AccountConfig {
    astra_account::load_str(
        "[imap]\naddr=\"imap.example.net:993\"\nserver_name=\"imap.example.net\"\n\
         username=\"info@test.local\"\npassword=\"p\"\n\
         \n[dav]\ncaldav_url=\"https://dav.example.net\"\n\
         username=\"info@test.local\"\npassword=\"p\"\n",
    )
    .expect("valid config")
}

/// Runs the dav fold over `account` with `dial` standing in for the real
/// connect, on one test's own runtime, boxed so each test hands a different
/// answer.
fn fold(
    config: &astra_account::AccountConfig,
    account: HostAccount,
    dial: Dial,
) -> (HostAccount, Option<HostContactsLeg>) {
    crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(move || dial_dav_with(config, account, move || async { dial() }))
}

#[test]
fn a_dav_dial_appends_the_calendar_leg_and_hands_back_the_contacts_leg() {
    let (account, contacts) = fold(
        &dav_config(),
        mail_account(),
        Box::new(|| {
            Ok(astra_account::DavProviders {
                caldav: Some(FakeDav::calendar()),
                carddav: Some(Box::new(FakeDav { calendars: false })),
            })
        }),
    );

    assert_eq!(
        account.providers.len(),
        2,
        "the calendar provider rides after the mail providers"
    );
    assert!(
        account.providers[1]
            .connection_info()
            .capabilities
            .calendars(),
        "the appended provider is the calendar one"
    );
    assert!(
        !account.providers[0]
            .connection_info()
            .capabilities
            .calendars(),
        "the mail half keeps its place in front"
    );
    assert!(
        contacts.is_some(),
        "the contacts leg is handed back for the install"
    );
}

#[test]
fn a_failed_dav_dial_leaves_the_mail_half_standing() {
    let (account, contacts) = fold(
        &dav_config(),
        mail_account(),
        Box::new(|| Err(astra_account::AccountError::Dav("unreachable".to_owned()))),
    );

    assert_eq!(
        account.providers.len(),
        1,
        "an unreachable dav origin costs the account neither its mail half nor the boot"
    );
    assert!(
        contacts.is_none(),
        "no contacts leg is installed over a failed dial"
    );
}

#[test]
fn an_account_without_a_dav_section_dials_nothing() {
    let config = astra_account::load_str(
        "[imap]\naddr=\"imap.example.net:993\"\nserver_name=\"imap.example.net\"\n\
         username=\"info@test.local\"\npassword=\"p\"\n",
    )
    .expect("valid config");
    let (account, contacts) = fold(
        &config,
        mail_account(),
        Box::new(|| panic!("a mail-only account dials no dav legs")),
    );

    assert_eq!(account.providers.len(), 1);
    assert!(contacts.is_none());
}

/// The boot assembles the MCP host off and exposing nothing: the state
/// every boot starts from, asserted at the assembly the panel reads.
/// Nothing of the mcp state persists, so this is not one boot's luck;
/// it is the only state a boot can hand back.
#[test]
fn the_assembly_hands_back_an_mcp_host_off_and_exposing_nothing() {
    let data_dir =
        std::env::temp_dir().join(format!("astra-desktop-boot-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&data_dir).expect("the scratch directory creates");
    let (_relay, agent_sink, _data) = crate::agent::testkit::stub_relay("boot-mcp", "echo", 3, 50);
    let state = super::assemble_state(
        &data_dir,
        std::sync::Arc::new(QuietObserver),
        agent_sink,
        std::sync::Arc::new(QuietDraftSink),
        crate::oauth::commands::offline_surface(),
    )
    .expect("the assembly boots over an empty store");

    assert_eq!(
        state.mcp.status(),
        crate::mcp::McpStatus::Off,
        "nothing listens until the panel turns the server on"
    );
    assert!(
        state.mcp.allowed().is_empty(),
        "no account is exposed until the panel names one"
    );
}

/// The assembly's observer: surface signals go unwatched, the probe below
/// reads the snapshot the dispatch leaves behind.
struct QuietObserver;

impl astra_app::AppObserver for QuietObserver {
    fn surface_changed(&self, _surface: astra_app::Surface) {
        // The probe pulls; the signal itself carries nothing it needs.
    }
}

/// The assembly's draft-pool sink: pool landings go unwatched.
struct QuietDraftSink;

impl astra_app::DraftPoolSink for QuietDraftSink {
    fn draft_pool_landed(&self, _message: astra_viewmodel::MessageKey) {}
}

#[test]
fn the_assembly_installs_the_host_account_store_before_the_state_hands_back() {
    // The probe: dispatch a SetDav whose merged section names no origin.
    // The installed seam refuses it itself, its validation running before
    // any dial, and the sentence lands on the account's row; a build with
    // no seam installed answers a different sentence of its own, so the
    // row's sentence pins the install.
    let data_dir =
        std::env::temp_dir().join(format!("astra-desktop-boot-ops-{}", std::process::id()));
    std::fs::create_dir_all(&data_dir).expect("the scratch directory creates");
    let (_relay, agent_sink, _data) = crate::agent::testkit::stub_relay("boot-ops", "echo", 3, 50);
    let state = super::assemble_state(
        &data_dir,
        std::sync::Arc::new(QuietObserver),
        agent_sink,
        std::sync::Arc::new(QuietDraftSink),
        crate::oauth::commands::offline_surface(),
    )
    .expect("the assembly boots over an empty store");

    let account = engine_api::AccountId::try_from("work").expect("an account id");
    let config = astra_account::load_str(concat!(
        "id = \"work\"\n",
        "[imap]\n",
        "addr = \"imap.example.net:993\"\n",
        "server_name = \"imap.example.net\"\n",
        "username = \"ada@example.test\"\n",
        "password = \"mail-secret\"\n",
    ))
    .expect("the fixture config loads");
    state
        .connections
        .register(&account, config, astra_account::ImapConnections::new())
        .expect("the dial table registers");

    state
        .runtime
        .block_on(|| {
            let app = std::sync::Arc::clone(&state.app);
            async move {
                app.add_new_account_deferred(super::HostAccount {
                    id: account.clone(),
                    providers: Vec::new(),
                    identity: engine_api::EmailAddress::new("ada@example.test"),
                })
                .await;
                app.dispatch(astra_app::Intent::Settings(
                    astra_app::SettingsIntent::SetDav {
                        account,
                        section: Some(astra_app::DavSectionInput::default()),
                    },
                ))
                .await
            }
        })
        .expect("the dispatch lands");

    let snapshot = state.app.settings();
    let row = snapshot
        .accounts
        .iter()
        .find(|row| row.account.as_str() == "work")
        .expect("the account's row stands");
    assert_eq!(
        row.dav_error.as_deref(),
        Some("the merged section names no origin to dial"),
        "the installed seam answered, not the no-seam fallback"
    );
}
