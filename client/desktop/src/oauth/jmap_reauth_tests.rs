// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the JMAP re-grant command surface: the begin mints the
//! authorization URL from the account's stored grant and opens it through
//! the one session's external arm (refusing honestly where the account
//! stores no grant at all), and the completion swaps the fresh grant in
//! place through the same account half a first sign-in runs: the standing
//! entry keeps its shape, the store's document carries the fresh grant,
//! the account re-dials, and a completion that answers a different
//! mailbox refuses with the stored grant untouched. No test touches a
//! network: the begin and the exchange are scripted seams, and the dial
//! is a stand-in.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use engine_api::{CalendarWrites, Capabilities, MailboxWrites, Provider};
use engine_provider::ConnectionInfo;
use tokio::sync::mpsc;

use astra_account::{AccountError, JmapAccount, JmapLoginStart};

use crate::app_state::HostProvider;
use crate::commands_fakes::{scratch, FakeApp, FakeStore};
use crate::dial_table::AccountDials;
use crate::oauth::commands::{OAuthCommandError, RefusalKind};

use super::{
    complete_jmap_regrant, jmap_reauth_with, JmapReauthBegin, JmapReauthExchange, JmapReauthHalves,
    JmapSignInOutcome, JmapSignInResult, SignInSurface,
};

/// A provider stand-in: the one account-wide JMAP provider.
struct FakeJmap;

impl FakeJmap {
    /// The account-wide provider's stand-in.
    fn provider() -> HostProvider {
        Box::new(Self)
    }
}

#[async_trait]
impl Provider for FakeJmap {
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::new(Capabilities::none().with_mail())
    }
}

impl MailboxWrites for FakeJmap {}
impl CalendarWrites for FakeJmap {}

/// The stored document a dead-granted account holds: the `[jmap]` section
/// with its grant, plus a dav section and an accepted certificate, the
/// two halves a re-grant must keep where they stood.
const STORED_DOCUMENT: &str = concat!(
    "[jmap]\n",
    "email = \"ada@example.test\"\n",
    "base_url = \"https://mail.example.test\"\n",
    "\n[jmap.oauth]\n",
    "client_id = \"client-xyz\"\n",
    "refresh_token = \"rt-dead\"\n",
    "authorize_endpoint = \"https://mail.example.test/oauth/authorize\"\n",
    "token_endpoint = \"https://mail.example.test/token\"\n",
    "redirect_uri = \"com.kenizhou.astra://oauth\"\n",
    "scopes = [\"offline_access\"]\n",
    "\n[dav]\n",
    "caldav_url = \"https://cal.example.test\"\n",
    "username = \"ada\"\n",
    "password = \"dav-pw\"\n",
);

/// The password-account document: a secret and no grant to re-run.
const PASSWORD_DOCUMENT: &str =
    "[jmap]\nemail = \"ada@example.test\"\nbase_url = \"https://mail.example.test\"\npassword = \"pw\"\n";

/// The account id the stored document derives.
const ACCOUNT: &str = "ada@example.test@jmap:mail.example.test";

/// The stored document's section, parsed.
fn stored_section() -> JmapAccount {
    astra_account::load_str(STORED_DOCUMENT)
        .expect("the stored document loads")
        .jmap
        .expect("the document is a jmap one")
}

/// The fresh section a scripted exchange answers: the same mailbox, a
/// fresh grant.
fn fresh_section() -> JmapAccount {
    astra_account::load_str(&STORED_DOCUMENT.replace("rt-dead", "rt-fresh"))
        .expect("the fresh document loads")
        .jmap
        .expect("the document is a jmap one")
}

/// A start the scripted begin seam mints, exactly the shape the account
/// layer's own begin answers.
fn minted_start() -> JmapLoginStart {
    JmapLoginStart {
        authorization_url: "https://mail.example.test/oauth/authorize?client_id=client-xyz"
            .to_owned(),
        pending: r#"{"email":"ada@example.test"}"#.to_owned(),
    }
}

/// A callback delivery on the OAuth scheme, raw as the OS hands it over.
const RAW_CALLBACK: &str = "com.kenizhou.astra://oauth?code=abc.def&state=xyz123";

/// A browser stand-in that records every URL it opened.
struct RecordingBrowser {
    urls: Mutex<Vec<String>>,
}

impl RecordingBrowser {
    /// A stand-in that opens.
    fn opening() -> Arc<Self> {
        Arc::new(Self {
            urls: Mutex::new(Vec::new()),
        })
    }

    /// The urls that opened.
    fn urls(&self) -> Vec<String> {
        self.urls.lock().unwrap().clone()
    }
}

impl crate::oauth::session::SystemBrowser for RecordingBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        self.urls.lock().unwrap().push(url.to_owned());
        Ok(())
    }
}

/// The surface over a browser stand-in and a capture, carrying no
/// registered client (the re-grant needs none).
fn surface(
    browser: Arc<RecordingBrowser>,
    callbacks: mpsc::UnboundedReceiver<String>,
) -> Arc<SignInSurface> {
    let tx = mpsc::unbounded_channel().0;
    Arc::new(SignInSurface::with_clients(
        browser,
        callbacks,
        tx,
        Vec::new(),
    ))
}

/// Feeds one raw callback into a fresh capture channel.
fn fed_capture() -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    tx.send(RAW_CALLBACK.to_owned())
        .expect("the fed delivery lands");
    rx
}

/// The begin seam answering the minted start.
fn minted_begin() -> JmapReauthBegin {
    Arc::new(|_section| Ok(minted_start()))
}

/// The begin seam that refuses: a password account's own refusal.
fn password_begin() -> JmapReauthBegin {
    Arc::new(|_section| {
        Err(AccountError::Jmap(
            "this jmap account signs in with a stored password, which has no sign-in to re-run"
                .to_owned(),
        ))
    })
}

/// The emit seam as the tests hold it: one channel the result lands in.
fn emit_seam() -> (
    std::sync::mpsc::Sender<JmapSignInResult>,
    std::sync::mpsc::Receiver<JmapSignInResult>,
    impl FnOnce(JmapSignInResult) + Send + 'static,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let sender = tx.clone();
    let emit = move |result: JmapSignInResult| {
        let _ = sender.send(result);
    };
    (tx, rx, emit)
}

/// Waits for the next result on the channel, bounded.
fn next_result(rx: &std::sync::mpsc::Receiver<JmapSignInResult>) -> JmapSignInResult {
    rx.recv_timeout(Duration::from_secs(5))
        .expect("the spawned half emitted its result")
}

/// The pieces over a fresh scratch index, with the dead-granted account
/// standing in the dial table and the store.
struct Host {
    app: Arc<FakeApp>,
    store: Arc<FakeStore>,
    dials: Arc<AccountDials>,
    refusals: Arc<crate::oauth::grant_refusals::GrantRefusals>,
    runtime: crate::runtime::AppRuntime,
    dialed: Arc<Mutex<Vec<String>>>,
}

impl Host {
    /// The pieces over a fresh scratch index holding the dead-granted
    /// account, whose refusal stands the way a dead grant leaves it.
    fn new(name: &str) -> Self {
        let config = astra_account::load_str(STORED_DOCUMENT).expect("the stored document loads");
        let dials = Arc::new(AccountDials::empty(&scratch(name)));
        dials
            .register(
                &engine_api::AccountId::try_from(ACCOUNT).unwrap(),
                config,
                astra_account::ImapConnections::new(),
            )
            .expect("the standing entry registers");
        let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());
        refusals.record_dead(ACCOUNT);
        Self {
            app: FakeApp::new(),
            store: FakeStore::accepting(),
            dials,
            refusals,
            runtime: crate::runtime::build_runtime().expect("the test runtime builds"),
            dialed: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The completion's wiring over these pieces.
    fn wiring(&self) -> super::JmapAccountWiring {
        super::JmapAccountWiring {
            app: Arc::clone(&self.app) as Arc<dyn crate::app_state::AppPort>,
            dials: Arc::clone(&self.dials),
            credentials: self.store.clone(),
            spawner: self.runtime.handle(),
            refusals: Arc::clone(&self.refusals),
        }
    }

    /// The dial stand-in, recording the account it dialled.
    fn dial(&self) -> super::JmapDial {
        let dialed = Arc::clone(&self.dialed);
        Arc::new(move |account, _tokens, _config| {
            let account = account.as_str().to_owned();
            let dialed = Arc::clone(&dialed);
            Box::pin(async move {
                dialed.lock().unwrap().push(account);
                Ok(FakeJmap::provider())
            })
        })
    }

    /// The exchange seam answering `fresh` for any redirect.
    fn exchange_with(fresh: JmapAccount) -> JmapReauthExchange {
        Arc::new(move |_stored, _pending, _callback| {
            let fresh = fresh.clone();
            Box::pin(async move { Ok(fresh) })
        })
    }

    /// The exchange seam that refuses, as a different-mailbox completion
    /// does.
    fn refusing_exchange(sentence: &'static str) -> JmapReauthExchange {
        Arc::new(move |_stored, _pending, _callback| {
            Box::pin(async move { Err(AccountError::Jmap(sentence.to_owned())) })
        })
    }

    /// Runs one completion over the given exchange, on the host's runtime.
    fn regrant(&self, exchange: JmapReauthExchange) -> JmapSignInOutcome {
        let wiring = self.wiring();
        let dial = self.dial();
        let stored = self.dials.of(ACCOUNT).and_then(|(config, _)| config.jmap);
        self.runtime.block_on(move || {
            complete_jmap_regrant(
                wiring,
                dial,
                exchange,
                stored,
                "pending".to_owned(),
                RAW_CALLBACK.to_owned(),
            )
        })
    }

    /// The stored document's refresh token, as the secure store holds it.
    fn stored_refresh_token(&self) -> String {
        let document = self
            .store
            .stored(ACCOUNT)
            .unwrap_or_else(|| panic!("the store holds the document for [{ACCOUNT}]"));
        astra_account::load_str(&document)
            .expect("the stored document loads")
            .jmap
            .expect("the document is a jmap one")
            .oauth
            .expect("the grant stands")
            .refresh_token
            .expose()
            .to_owned()
    }
}

#[test]
fn a_begun_regrant_opens_the_stored_grant_s_authorization_url() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), fed_capture());
    let (_tx, results, emit) = emit_seam();

    jmap_reauth_with(
        &desk,
        &runtime,
        emit,
        JmapReauthHalves {
            begin: minted_begin(),
            complete: Box::new(|_pending, _callback| {
                Box::pin(async move {
                    JmapSignInOutcome::Added {
                        address: "ada@example.test".to_owned(),
                    }
                })
            }),
        },
        &stored_section(),
    )
    .expect("the browser opened the stored grant's url");

    assert_eq!(
        browser.urls(),
        vec![minted_start().authorization_url],
        "the url that opened is the url the stored grant minted"
    );
    let result = next_result(&results);
    assert_eq!(
        result.outcome,
        JmapSignInOutcome::Added {
            address: "ada@example.test".to_owned()
        },
        "the re-grant's outcome crosses the emit seam"
    );
}

#[test]
fn a_password_account_s_regrant_refuses_and_opens_nothing() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let browser = RecordingBrowser::opening();
    let desk = surface(Arc::clone(&browser), fed_capture());
    let (_tx, _results, emit) = emit_seam();

    let err = jmap_reauth_with(
        &desk,
        &runtime,
        emit,
        JmapReauthHalves {
            begin: password_begin(),
            complete: Box::new(|_pending, _callback| {
                Box::pin(async move {
                    JmapSignInOutcome::Added {
                        address: "ada@example.test".to_owned(),
                    }
                })
            }),
        },
        &astra_account::load_str(PASSWORD_DOCUMENT)
            .expect("the password document loads")
            .jmap
            .expect("the document is a jmap one"),
    )
    .expect_err("a password account has no sign-in to re-run");

    assert!(
        matches!(err, OAuthCommandError::NoGrant(ref said) if said.contains("password")),
        "the refusal names the password form: {err}"
    );
    assert!(
        browser.urls().is_empty(),
        "a refused begin opens no browser"
    );
}

#[test]
fn the_regrant_swaps_the_grant_in_place_and_keeps_what_stood() {
    let host = Host::new("regrant-swap");

    let outcome = host.regrant(Host::exchange_with(fresh_section()));

    assert_eq!(
        outcome,
        JmapSignInOutcome::Added {
            address: "ada@example.test".to_owned()
        },
        "the re-grant completed into the standing account"
    );
    assert_eq!(
        host.stored_refresh_token(),
        "rt-fresh",
        "the store's document now holds the fresh grant"
    );
    let (config, _) = host.dials.of(ACCOUNT).expect("the entry stands");
    assert_eq!(
        config.dav.as_ref().and_then(|dav| dav.caldav_url.clone()),
        Some("https://cal.example.test".to_owned()),
        "the dav section the grant does not own stays where it stood"
    );
    let dialed = host.dialed.lock().unwrap();
    assert_eq!(
        *dialed,
        vec![ACCOUNT.to_owned()],
        "the account re-dialled on its fresh grant"
    );
    assert!(
        host.refusals.of(ACCOUNT).is_none(),
        "the dialled grant clears the standing refusal"
    );
}

#[test]
fn a_different_mailbox_completion_refuses_and_the_stored_grant_stands() {
    let host = Host::new("regrant-mismatch");

    let outcome = host.regrant(Host::refusing_exchange(
        "that sign-in is for a different account than the one being reconnected",
    ));

    match outcome {
        JmapSignInOutcome::Refused { refusal, detail } => {
            assert_eq!(refusal, RefusalKind::Indeterminate);
            assert!(
                detail.contains("different account"),
                "the account layer's own sentence rides: {detail}"
            );
        }
        other @ JmapSignInOutcome::Added { .. } => {
            panic!("the mismatch refused, got {other:?}")
        }
    }
    assert!(
        host.store.stored(ACCOUNT).is_none() || host.stored_refresh_token() == "rt-dead",
        "the stored grant stands untouched: {:?}",
        host.store.stored(ACCOUNT)
    );
    assert!(
        host.dialed.lock().unwrap().is_empty(),
        "a refused completion dials nothing"
    );
    assert!(
        host.refusals.of(ACCOUNT).is_some(),
        "the standing refusal stays until a re-grant lands"
    );
}
