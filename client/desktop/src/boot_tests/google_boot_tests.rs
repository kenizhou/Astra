// SPDX-License-Identifier: GPL-3.0-only
//! The google account's boot half: a stored `[google]` document dials
//! through the same free functions the graph half dials through (its
//! calendar appended after the account-global mail provider), a dead
//! grant skips the account and leaves the expired-sign-in state the
//! accounts row reads, and the folder binder binds a Google account's
//! folders over its stored grant.

use std::sync::{Arc, Mutex};

use super::FakeDav;

/// A google dial seam that is never meant to run: the graph boot tests
/// hand it for accounts their tables do not hold.
pub(super) fn unused_dial() -> crate::oauth::google_add::GoogleDial {
    Arc::new(|_account, _tokens, _config| {
        Box::pin(async move { panic!("a graph-only boot dialled a google account") })
    })
}

/// A graph dial seam that is never meant to run: the google boot tests
/// and the google lifecycle proof hand it for accounts their tables do
/// not hold.
pub(super) fn unused_graph_dial() -> crate::oauth::graph_add::GraphDial {
    Arc::new(|_account, _tokens, _config| {
        Box::pin(async move { panic!("a google-only boot dialled a graph account") })
    })
}

/// A stored google-only document: the shape the sign-in's completion
/// persists.
fn google_config() -> astra_account::AccountConfig {
    astra_account::load_str(
        "[google]\nemail = \"alice@gmail.example\"\nclient_id = \"google-client\"\n\
         client_secret = \"gsecret\"\nredirect_uri = \"http://127.0.0.1:45001/\"\n\
         scopes = [\"https://mail.google.com/\"]\nrefresh_token = \"old-refresh\"\n",
    )
    .expect("valid google config")
}

/// What the boot's google dial seam answers.
#[derive(Clone, Copy)]
enum Answer {
    /// A dialled account with its account-global mail and its calendar.
    Healthy,
    /// A grant the token endpoint refused.
    DeadGrant,
}

/// The google dial seam over one scripted answer, recording the refresh
/// token the boot built its token source over.
fn google_dial(
    grants: Arc<Mutex<Vec<String>>>,
    answer: Answer,
) -> crate::oauth::google_add::GoogleDial {
    Arc::new(move |_account, _tokens, config| {
        let seen_refresh = config
            .google
            .as_ref()
            .map(|google| google.refresh_token.expose().to_owned())
            .unwrap_or_default();
        let grants = Arc::clone(&grants);
        Box::pin(async move {
            grants.lock().unwrap().push(seen_refresh);
            match answer {
                Answer::Healthy => Ok(astra_account::GoogleProviders {
                    mail: vec![FakeDav::mail()],
                    calendar: Some(FakeDav::calendar()),
                    calendar_error: None,
                }),
                // The dead word the real token source answers for a grant
                // the server refused; a stored document's endpoint is
                // pinned to the real Google authority, so the boot leg
                // scripts the arrival.
                Answer::DeadGrant => Err(astra_account::AccountError::SigninRejected(
                    "the refresh token was refused".to_owned(),
                )),
            }
        })
    })
}

/// Runs one boot's stored-account dial over `dials` with the given
/// google dial and the refusal map the boot records into, on a test
/// runtime.
fn boot_dials_google(
    dials: &Arc<crate::AccountDials>,
    dial: crate::oauth::google_add::GoogleDial,
    refusals: &Arc<crate::oauth::grant_refusals::GrantRefusals>,
) -> Vec<super::super::stored::Dialled> {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let store: Arc<dyn crate::credential_store::AccountCredentialStore> =
        crate::commands_fakes::FakeStore::accepting();
    let dials = Arc::clone(dials);
    let refusals = Arc::clone(refusals);
    runtime.block_on(move || async move {
        super::super::stored::dial_stored(&dials, &store, unused_graph_dial(), dial, &refusals)
            .await
    })
}

/// A registered google account under one scratch index, the fixture the
/// boot tests share.
fn registered_google_account(name: &str) -> Arc<crate::AccountDials> {
    let dials = Arc::new(crate::AccountDials::empty(&crate::commands_fakes::scratch(
        name,
    )));
    let account = engine_api::AccountId::try_from("alice@gmail.example@google").unwrap();
    dials
        .register(
            &account,
            google_config(),
            astra_account::ImapConnections::new(),
        )
        .expect("the google account registers");
    dials
}

#[test]
fn a_stored_google_document_dials_at_boot_with_its_calendar_appended() {
    let dials = registered_google_account("boot-google");
    let grants = Arc::new(Mutex::new(Vec::new()));
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());

    let dialled = boot_dials_google(
        &dials,
        google_dial(Arc::clone(&grants), Answer::Healthy),
        &refusals,
    );

    assert_eq!(dialled.len(), 1, "the stored google account dials at boot");
    let account = &dialled[0].account;
    assert_eq!(account.id.as_str(), "alice@gmail.example@google");
    assert_eq!(account.identity.email, "alice@gmail.example");
    assert_eq!(
        account.providers.len(),
        2,
        "the account-global mail provider stands and the calendar appends after it"
    );
    assert!(
        !account.providers[0]
            .connection_info()
            .capabilities
            .calendars(),
        "the mail half keeps its place in front"
    );
    assert!(
        account.providers[1]
            .connection_info()
            .capabilities
            .calendars(),
        "the appended provider is the calendar one"
    );
    assert_eq!(
        grants.lock().unwrap().as_slice(),
        ["old-refresh"],
        "the boot dialled over the grant the document stores"
    );
    assert_eq!(
        refusals.of("alice@gmail.example@google"),
        None,
        "a healthy dial crosses no refusal state"
    );
}

#[test]
fn a_dead_google_grant_at_boot_records_the_expired_sign_in_and_skips() {
    let dials = registered_google_account("boot-google-dead");
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());

    let dialled = boot_dials_google(
        &dials,
        google_dial(Arc::new(Mutex::new(Vec::new())), Answer::DeadGrant),
        &refusals,
    );

    assert!(dialled.is_empty(), "the account never joins the app");
    assert_eq!(
        refusals.of("alice@gmail.example@google"),
        Some(astra_viewmodel::GrantState::SignInExpired),
        "the dead google grant reaches the accounts row, never an outage badge"
    );
}

/// The folder binder's google arm: any mailbox key binds the
/// account-global Gmail provider over the stored grant, because Gmail
/// has no per-folder binding.
#[test]
fn the_folder_binder_binds_a_google_account_s_account_global_provider() {
    let dials = registered_google_account("boot-google-folder");
    let connector = super::super::stored::DialConnector::new(Arc::clone(&dials));
    let account = engine_api::AccountId::try_from("alice@gmail.example@google").unwrap();

    let bound = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(move || async move {
            astra_app::MailboxConnector::connect_folder(&connector, &account, "label/whatever")
                .await
        });

    let provider = bound.expect("the binder answers over the stored google grant");
    assert!(
        provider.connection_info().capabilities.mail(),
        "the bound provider is the account's mail half"
    );
}
