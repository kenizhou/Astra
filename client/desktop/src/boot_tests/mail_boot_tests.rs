// SPDX-License-Identifier: GPL-3.0-only
//! The mail oauth account's boot half: a stored `[imap.oauth]` grant
//! document dials at boot through the sink-backed source (a rotation the
//! ratcheting server issues at that dial lands in the entry and the
//! store, the defence without which a dropped rotation kills the account
//! at the next launch), a dead grant records the expired-sign-in state
//! the imap_smtp row renders and skips the account, and a password
//! account's boot never consults the grant dial. The token endpoint is
//! a scripted loopback HTTP server: the real production stack makes the
//! requests. The dial seam is scripted; its mint through the real
//! source is what the account layer's dial tests proved.

use std::sync::{Arc, Mutex};

use astra_account::AccountConfig;

use super::FakeDav;

/// One scripted answer: the status line and the body.
type Answer = (&'static str, String);

/// Reads a whole small HTTP request, honouring `Content-Length`.
fn read_request(stream: &mut std::net::TcpStream) -> String {
    use std::io::Read;
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

/// Writes one answer as a whole HTTP response.
fn respond(stream: &mut std::net::TcpStream, answer: Answer) {
    use std::io::Write;
    let (status, body) = answer;
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    );
    let _ = stream.write_all(response.as_bytes());
}

/// A rotating, ratcheting token endpoint: every refresh presenting the
/// currently valid token rotates it and succeeds, and anything else
/// revokes the grant for good. The one server shape that makes the
/// boot's rotation landing observable.
fn rotating_token(request: &str) -> Answer {
    static VALID: Mutex<Option<String>> = Mutex::new(None);
    let mut valid = VALID.lock().expect("the ratchet lock stands");
    let presented = form_value(request, "refresh_token");
    let expected = valid.clone().unwrap_or_else(|| "rt-1".to_owned());
    if presented != expected {
        *valid = None;
        return (
            "400 Bad Request",
            r#"{"error":"invalid_grant","error_description":"replayed"}"#.to_owned(),
        );
    }
    let generation = expected.matches("rotated").count() + 1;
    let next = format!("rotated-{generation}");
    let answer = (
        "200 OK",
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
        r#"{"error":"invalid_grant","error_description":"revoked"}"#.to_owned(),
    )
}

/// The one server every mail boot test talks to. Returns its base URL.
fn token_server(token: fn(&str) -> Answer) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let request = read_request(&mut stream);
            respond(&mut stream, token(&request));
        }
    });
    format!("http://{addr}")
}

/// A stored `[imap.oauth]` grant document for `host`, whose token
/// endpoint is `base`, the shape the sign-in's completion persists: no
/// password anywhere. Each test names its own host, because the account
/// id derives from it and the token state is shared per account in this
/// process.
fn grant_config(base: &str, host: &str) -> AccountConfig {
    astra_account::load_str(&format!(
        "[imap]\naddr = \"{host}:993\"\nserver_name = \"{host}\"\n\
         username = \"alice@example.test\"\n\
         \n[imap.oauth]\nclient_id = \"client-abc\"\nrefresh_token = \"rt-1\"\n\
         authorize_endpoint = \"{base}/authorize\"\ntoken_endpoint = \"{base}/token\"\n\
         redirect_uri = \"com.kenizhou.astra://oauth\"\nscopes = [\"offline_access\"]\n"
    ))
    .expect("valid mail grant config")
}

/// A stored `[imap]` password document at an unreachable server: the
/// shape every boot before this family dialled.
fn password_config() -> AccountConfig {
    astra_account::load_str(concat!(
        "[imap]\naddr = \"unreachable.example.test:993\"\n",
        "server_name = \"unreachable.example.test\"\n",
        "username = \"alice@example.test\"\npassword = \"pw\"\n",
    ))
    .expect("valid password config")
}

/// The account id a mail grant document for `email` at `host` derives.
fn mail_id(email: &str, host: &str) -> String {
    format!("{}@{}", email.trim().to_lowercase(), host)
}

/// Registers one stored mail document under one scratch index, answering
/// the id its host derives.
fn registered(
    name: &str,
    config: AccountConfig,
    email: &str,
    host: &str,
) -> Arc<crate::AccountDials> {
    let dials = Arc::new(crate::AccountDials::empty(&crate::commands_fakes::scratch(
        name,
    )));
    let id = engine_api::AccountId::try_from(mail_id(email, host).as_str()).unwrap();
    dials
        .register(&id, config, astra_account::ImapConnections::new())
        .expect("the mail account registers");
    dials
}

/// Runs one boot's stored-account dial over `dials`, `store`, and
/// `refusals`, on a test runtime, with the mail dial seam `dial`.
fn boot_dials(
    dials: &Arc<crate::AccountDials>,
    store: Arc<dyn crate::credential_store::AccountCredentialStore>,
    dial: crate::oauth::mail_add::MailGrantDial,
    refusals: &Arc<crate::oauth::grant_refusals::GrantRefusals>,
) -> Vec<super::super::stored::Dialled> {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let dials = Arc::clone(dials);
    let refusals = Arc::clone(refusals);
    runtime.block_on(move || async move {
        super::super::stored::dial_stored(
            &dials,
            &store,
            &crate::commands_fakes::crypto_store(),
            unused_graph_dial(),
            unused_google_dial(),
            dial,
            &refusals,
        )
        .await
    })
}

/// A graph dial seam that is never meant to run.
fn unused_graph_dial() -> crate::oauth::graph_add::GraphDial {
    Arc::new(|_account, _tokens, _config| {
        Box::pin(async move { panic!("a mail-only boot dialled a graph account") })
    })
}

/// A google dial seam that is never meant to run.
fn unused_google_dial() -> crate::oauth::google_add::GoogleDial {
    Arc::new(|_account, _tokens, _config| {
        Box::pin(async move { panic!("a mail-only boot dialled a google account") })
    })
}

/// The dial seam that mints through the real source it was handed, as
/// the production dial does before it connects, then answers the bound
/// folders: the mint is what spends the refresh token, and a rotating
/// server answers it with a rotation the sink must land.
fn minting_dial() -> crate::oauth::mail_add::MailGrantDial {
    Arc::new(|_account, tokens, _config, _pool| {
        Box::pin(async move {
            let _token = tokens.access_token().await?;
            Ok(vec![FakeDav::mail(), FakeDav::mail()])
        })
    })
}

#[test]
fn a_stored_mail_grant_dials_at_boot_and_lands_its_rotation() {
    let base = token_server(rotating_token);
    let dials = registered(
        "boot-mail-grant",
        grant_config(&base, "mail.example.test"),
        "alice@example.test",
        "mail.example.test",
    );
    let store = crate::commands_fakes::FakeStore::accepting();
    let id = mail_id("alice@example.test", "mail.example.test");
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());

    let dialled = boot_dials(&dials, store.clone(), minting_dial(), &refusals);

    assert_eq!(dialled.len(), 1, "the grant account dials at boot");
    let account = &dialled[0].account;
    assert_eq!(account.id.as_str(), id);
    assert_eq!(account.identity.email, "alice@example.test");
    assert_eq!(
        account.providers.len(),
        2,
        "the dialled account joins with its bound folders"
    );
    let entry = dials.of(&id).expect("the entry stands").0;
    let advanced = entry
        .imap
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
    let stored = store.stored(&id).expect("the rotated grant persisted");
    let parsed = astra_account::load_str(&stored).expect("the stored document loads");
    assert_eq!(
        parsed
            .imap
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
fn a_dead_mail_grant_at_boot_records_the_expired_sign_in_and_skips() {
    let base = token_server(dead_token);
    let dials = registered(
        "boot-mail-dead",
        grant_config(&base, "dead.example.test"),
        "alice@example.test",
        "dead.example.test",
    );
    let id = mail_id("alice@example.test", "dead.example.test");
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());

    let dialled = boot_dials(
        &dials,
        crate::commands_fakes::FakeStore::accepting(),
        minting_dial(),
        &refusals,
    );

    assert!(dialled.is_empty(), "the account never joins the app");
    assert_eq!(
        refusals.of(&id),
        Some(astra_viewmodel::GrantState::SignInExpired),
        "the dead mail grant reaches the imap_smtp row, never an outage badge"
    );
}

#[test]
fn a_password_account_s_boot_never_consults_the_grant_dial() {
    // The password arm is unchanged: an `[imap]` document without a
    // grant dials through the standing password path, and the grant dial
    // seam never runs for it.
    let dials = registered(
        "boot-mail-password",
        password_config(),
        "alice@example.test",
        "unreachable.example.test",
    );
    let panicking: crate::oauth::mail_add::MailGrantDial =
        Arc::new(|_account, _tokens, _config, _pool| {
            Box::pin(async move { panic!("a password account's boot consulted the grant dial") })
        });
    let refusals = Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());

    let dialled = boot_dials(
        &dials,
        crate::commands_fakes::FakeStore::accepting(),
        panicking,
        &refusals,
    );

    // The unreachable server skips the account with a warning; the test
    // completing at all is the proof the grant dial never ran.
    assert!(dialled.is_empty());
    assert!(
        refusals
            .of(&mail_id("alice@example.test", "unreachable.example.test"))
            .is_none(),
        "a password refusal records no grant state"
    );
}
