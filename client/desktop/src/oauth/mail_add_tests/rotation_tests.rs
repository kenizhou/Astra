// SPDX-License-Identifier: GPL-3.0-only
//! The entry-before-dial ordering's proof: a rotation that happens
//! during the dial lands in the sink, and the grant the completion
//! persists is the advanced one, never the copy parsed before the dial.
//! Its own file for the mock rotating token endpoint, which is the
//! piece that makes the ordering observable offline.

use super::*;

/// A rotating, ratcheting token endpoint: the code exchange answers the
/// pair the grant starts from, and every refresh presenting the
/// currently valid token rotates it, the shape that makes the
/// entry-before-dial ordering and the read-back persist observable
/// offline.
fn rotating_token_endpoint(initial: &str) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    let mut valid = initial.to_owned();
    let mut generation = 0usize;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let request = read_request(&mut stream);
            let (status, body) = if request.contains("grant_type=authorization_code") {
                (
                    "200 OK",
                    format!(
                        r#"{{"token_type":"Bearer","expires_in":3600,"access_token":"AT-0","refresh_token":"{valid}","scope":"offline_access"}}"#
                    ),
                )
            } else {
                let presented = request
                    .split_once("\r\n\r\n")
                    .and_then(|(_, body)| {
                        body.split('&')
                            .find_map(|pair| pair.strip_prefix("refresh_token="))
                    })
                    .unwrap_or_default()
                    .to_owned();
                if presented == valid && !valid.is_empty() {
                    generation += 1;
                    valid = format!("rotated-{generation}");
                    (
                        "200 OK",
                        format!(
                            r#"{{"token_type":"Bearer","expires_in":3600,"access_token":"AT-{generation}","refresh_token":"{valid}"}}"#
                        ),
                    )
                } else {
                    valid.clear();
                    (
                        "400 Bad Request",
                        r#"{"error":"invalid_grant","error_description":"replayed"}"#.to_owned(),
                    )
                }
            };
            respond(&mut stream, (status, body));
        }
    });
    format!("http://{addr}/token")
}

/// The dial seam that mints through the source it was handed, as the
/// production dial does before it connects: the mint is what spends the
/// refresh token, and a rotating server answers it with a rotation the
/// sink must land while the dial still runs.
fn minting_dial(saw: Arc<DialSaw>) -> MailGrantDial {
    dial_seam(saw, |tokens| async move {
        let _token = tokens.access_token().await?;
        Ok(vec![FakeFolder::provider()])
    })
}

#[test]
fn a_rotation_during_the_dial_lands_and_the_persist_keeps_it() {
    let host = Host::new("mail-add-rotation");
    let endpoint = rotating_token_endpoint("rt-9");

    complete(
        &host,
        exchange(pending(&endpoint)).expect("the exchange answers over the rotating server"),
        minting_dial(Arc::new(DialSaw::default())),
    )
    .expect("the sign-in completes over the rotating server");

    let id = mail_id("alice@example.test");
    // The rotation the mint triggered landed while the dial ran, through
    // the sink, into the entry that stood before the dial began.
    let imap = stored_imap(&host, &id);
    assert_eq!(
        imap.oauth.expect("the grant").refresh_token.expose(),
        "rotated-1",
        "the grant that persists is the advanced one, never the copy parsed before the dial"
    );
    assert!(
        stored_documents(&host, &id).len() >= 2,
        "the rotation's own write landed beside the add's"
    );
    let entry = host.dials.of(&id).expect("the entry stands").0;
    assert_eq!(
        entry
            .imap
            .expect("the imap section")
            .oauth
            .expect("the grant")
            .refresh_token
            .expose(),
        "rotated-1",
        "the dial table's entry carries the advanced grant"
    );
}
