// SPDX-License-Identifier: GPL-3.0-only
//! The jmap account's boot half: a stored `[jmap]` password document
//! dials its one account-wide provider at boot, a stored grant dials
//! through the sink-backed token source (a rotation the ratcheting
//! server issues at that dial lands in the entry and the store, the
//! defence without which a dropped rotation kills the account at the
//! next launch), a dead grant records the expired-sign-in state and
//! skips the account, and the folder binder reconnects the same
//! account-wide provider for any mailbox key, never a per-folder one.
//! The servers are scripted loopback HTTP: the real production stack
//! makes the requests.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// A minimal session document advertising mail for one account, served
/// by a server at `port`.
fn session_body(port: u16) -> String {
    let mail = "urn:ietf:params:jmap:mail";
    format!(
        "{{\"apiUrl\":\"http://127.0.0.1:{port}/api\",\
         \"capabilities\":{{\"urn:ietf:params:jmap:core\":{{}},\"{mail}\":{{}}}},\
         \"primaryAccounts\":{{\"{mail}\":\"A1\"}},\
         \"accounts\":{{\"A1\":{{\"name\":\"scripted\",\"accountCapabilities\":{{\"{mail}\":{{}}}}}}}}}}",
    )
}

/// Reads a whole small HTTP request, honouring `Content-Length`, so a
/// POST body split from its headers still arrives whole.
fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(read) = stream.read(&mut buf) {
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&buf[..read]);
        let text = String::from_utf8_lossy(&raw);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length: usize = head
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length: ")
                        .or_else(|| line.strip_prefix("content-length: "))
                })
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(0);
            if body.len() >= length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&raw).into_owned()
}

/// One `application/x-www-form-urlencoded` value out of a request body.
fn form_value(request: &str, key: &str) -> String {
    request
        .split_once("\r\n\r\n")
        .and_then(|(_, body)| {
            body.split('&')
                .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        })
        .unwrap_or_default()
        .to_owned()
}

/// One scripted answer: the status line, extra headers, and the body.
type Answer = (&'static str, String, String);

/// Writes one answer as a whole HTTP response.
fn respond(stream: &mut std::net::TcpStream, answer: Answer) {
    let (status, headers, body) = answer;
    let response = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    );
    let _ = stream.write_all(response.as_bytes());
}

/// A token endpoint that answers a static fresh pair, whatever it was
/// handed.
fn static_token(_request: &str) -> Answer {
    (
        "200 OK",
        String::new(),
        r#"{"token_type":"Bearer","expires_in":3600,"access_token":"AT-1","refresh_token":"rt-2","scope":"offline_access"}"#
            .to_owned(),
    )
}

/// A rotating, ratcheting token endpoint: exactly one currently-valid
/// refresh token, a refresh presenting it rotates and succeeds, and a
/// refresh presenting anything else revokes the grant for good. The one
/// server shape that makes the boot's rotation landing observable.
fn rotating_token(request: &str) -> Answer {
    static VALID: Mutex<Option<String>> = Mutex::new(None);
    let mut valid = VALID.lock().expect("the ratchet lock stands");
    let presented = form_value(request, "refresh_token");
    let expected = valid.clone().unwrap_or_else(|| "rt-1".to_owned());
    if presented != expected {
        *valid = None;
        return (
            "400 Bad Request",
            String::new(),
            r#"{"error":"invalid_grant","error_description":"replayed"}"#.to_owned(),
        );
    }
    let generation = expected.matches("rotated").count() + 1;
    let next = format!("rotated-{generation}");
    let answer = (
        "200 OK",
        String::new(),
        format!(
            r#"{{"token_type":"Bearer","expires_in":3600,"access_token":"AT-{generation}","refresh_token":"{next}"}}"#
        ),
    );
    *valid = Some(next);
    answer
}

/// A token endpoint that refuses every refresh with `invalid_grant`.
fn dead_token(_request: &str) -> Answer {
    (
        "400 Bad Request",
        String::new(),
        r#"{"error":"invalid_grant","error_description":"revoked"}"#.to_owned(),
    )
}

/// The one server every jmap boot test talks to: the session document on
/// the well-known path and the token endpoint beside it, both plain
/// loopback HTTP. Returns the base URL.
fn jmap_server(token: fn(&str) -> Answer) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    std::thread::spawn(move || {
        let body = session_body(addr.port());
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let request = read_request(&mut stream);
            let path = request
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .split('?')
                .next()
                .unwrap_or("/")
                .to_owned();
            let answer = match path.as_str() {
                "/.well-known/jmap" => ("200 OK", String::new(), body.clone()),
                "/token" => token(&request),
                _ => continue,
            };
            respond(&mut stream, answer);
        }
    });
    format!("http://{addr}")
}

/// A stored `[jmap]` password document at `base`.
fn password_config(base: &str) -> astra_account::AccountConfig {
    astra_account::load_str(&format!(
        "[jmap]\nemail = \"alice@example.test\"\nbase_url = \"{base}\"\npassword = \"pw\"\n"
    ))
    .expect("valid jmap config")
}

/// A stored `[jmap]` grant document at `base`, the shape the sign-in's
/// completion persists.
fn grant_config(base: &str) -> astra_account::AccountConfig {
    astra_account::load_str(&format!(
        "[jmap]\nemail = \"alice@example.test\"\nbase_url = \"{base}\"\n\n[jmap.oauth]\nclient_id = \"client-abc\"\nrefresh_token = \"rt-1\"\nauthorize_endpoint = \"{base}/oauth/authorize\"\ntoken_endpoint = \"{base}/token\"\nredirect_uri = \"com.kenizhou.astra://oauth\"\nscopes = [\"offline_access\"]\n"
    ))
    .expect("valid jmap grant config")
}

/// Registers one stored jmap document under one scratch index, answering
/// the account id its base URL derives.
fn registered(
    name: &str,
    config: astra_account::AccountConfig,
    base: &str,
) -> (Arc<crate::AccountDials>, engine_api::AccountId) {
    let dials = Arc::new(crate::AccountDials::empty(&crate::commands_fakes::scratch(
        name,
    )));
    let host = base
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(base);
    let id = engine_api::AccountId::try_from(format!("alice@example.test@jmap:{host}").as_str())
        .unwrap();
    dials
        .register(&id, config, astra_account::ImapConnections::new())
        .expect("the jmap account registers");
    (dials, id)
}

/// Runs one boot's stored-account dial over `dials` and `refusals`, on a
/// test runtime, over the given credential store.
fn boot_dials(
    dials: &Arc<crate::AccountDials>,
    store: Arc<dyn crate::credential_store::AccountCredentialStore>,
    refusals: &Arc<crate::oauth::grant_refusals::GrantRefusals>,
) -> Vec<super::super::stored::Dialled> {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let dials = Arc::clone(dials);
    let refusals = Arc::clone(refusals);
    runtime.block_on(move || async move {
        super::super::stored::dial_stored(
            &dials,
            &store,
            unused_graph_dial(),
            unused_google_dial(),
            &refusals,
        )
        .await
    })
}

/// A graph dial seam that is never meant to run.
fn unused_graph_dial() -> crate::oauth::graph_add::GraphDial {
    Arc::new(|_account, _tokens, _config| {
        Box::pin(async move { panic!("a jmap-only boot dialled a graph account") })
    })
}

/// A google dial seam that is never meant to run.
fn unused_google_dial() -> crate::oauth::google_add::GoogleDial {
    Arc::new(|_account, _tokens, _config| {
        Box::pin(async move { panic!("a jmap-only boot dialled a google account") })
    })
}

#[test]
fn a_stored_jmap_password_document_dials_its_one_provider_at_boot() {
    let base = jmap_server(static_token);
    let (dials, id) = registered("boot-jmap-password", password_config(&base), &base);
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());

    let dialled = boot_dials(
        &dials,
        crate::commands_fakes::FakeStore::accepting(),
        &refusals,
    );

    assert_eq!(dialled.len(), 1, "the jmap account dials at boot");
    let account = &dialled[0].account;
    assert_eq!(
        account.id.as_str(),
        id.as_str(),
        "the id names the address under the jmap scheme and the server host"
    );
    assert_eq!(account.identity.email, "alice@example.test");
    assert_eq!(
        account.providers.len(),
        1,
        "one account-wide provider serves the whole account"
    );
    assert!(
        account.providers[0].connection_info().capabilities.mail(),
        "the bound provider is the account's mail half"
    );
}

#[test]
fn the_folder_binder_reconnects_the_account_wide_provider_for_any_key() {
    let base = jmap_server(static_token);
    let (dials, id) = registered("boot-jmap-folder", password_config(&base), &base);
    let connector = super::super::stored::DialConnector::new(Arc::clone(&dials));

    let bound = crate::runtime::build_runtime()
        .expect("the test runtime builds")
        .block_on(move || async move {
            // Two different mailbox keys, two binds: the account-wide
            // provider answers both, because the key names nothing.
            let inbox = astra_app::MailboxConnector::connect_folder(&connector, &id, "inbox")
                .await
                .expect("the first key binds");
            let archive = astra_app::MailboxConnector::connect_folder(&connector, &id, "archive")
                .await
                .expect("the second key binds the same account-wide provider");
            (inbox, archive)
        });

    for provider in [&bound.0, &bound.1] {
        assert!(
            provider.connection_info().capabilities.mail(),
            "every bind answers the account-wide provider"
        );
    }
}

#[test]
fn a_stored_jmap_grant_dials_at_boot_and_lands_its_rotation() {
    let base = jmap_server(rotating_token);
    let (dials, id) = registered("boot-jmap-grant", grant_config(&base), &base);
    let store = crate::commands_fakes::FakeStore::accepting();
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());

    let dialled = boot_dials(&dials, store.clone(), &refusals);

    assert_eq!(dialled.len(), 1, "the grant account dials at boot");
    let entry = dials.of(id.as_str()).expect("the entry stands").0;
    let advanced = entry
        .jmap
        .expect("the section")
        .oauth
        .expect("the grant")
        .refresh_token
        .expose()
        .to_owned();
    assert_ne!(
        advanced, "rt-1",
        "the rotation the boot dial triggered landed in the entry"
    );
    let stored = store
        .stored(id.as_str())
        .expect("the rotated grant persisted");
    let parsed = astra_account::load_str(&stored).expect("the stored document loads");
    assert_eq!(
        parsed
            .jmap
            .expect("the section")
            .oauth
            .expect("the grant")
            .refresh_token
            .expose(),
        advanced,
        "the store carries the advanced grant, so the next launch presents it"
    );
}

#[test]
fn a_dead_jmap_grant_at_boot_records_the_expired_sign_in_and_skips() {
    let base = jmap_server(dead_token);
    let (dials, id) = registered("boot-jmap-dead", grant_config(&base), &base);
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());

    let dialled = boot_dials(
        &dials,
        crate::commands_fakes::FakeStore::accepting(),
        &refusals,
    );

    assert!(dialled.is_empty(), "the account never joins the app");
    assert_eq!(
        refusals.of(id.as_str()),
        Some(astra_viewmodel::GrantState::SignInExpired),
        "the dead jmap grant reaches the accounts row, never an outage badge"
    );
}
