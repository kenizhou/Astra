// SPDX-License-Identifier: GPL-3.0-only
//! The entry-before-dial ordering's proof: a rotation that happens
//! during the dial lands in the sink, and the grant the completion
//! persists is the advanced one, never the copy parsed before the dial.
//! Its own file for the mock rotating token endpoint, which is the
//! piece that makes the ordering observable offline.

use super::*;

/// A rotating token endpoint: every refresh presenting the currently
/// valid token rotates it, the shape that makes the entry-before-dial
/// ordering and the read-back persist observable offline.
fn rotating_token_endpoint(initial: &str) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let addr = listener.local_addr().expect("a bound address");
    let mut valid = initial.to_owned();
    let mut generation = 0usize;
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
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
            let request = String::from_utf8_lossy(&raw).into_owned();
            let presented = request
                .split_once("\r\n\r\n")
                .and_then(|(_, body)| {
                    body.split('&')
                        .find_map(|pair| pair.strip_prefix("refresh_token="))
                })
                .unwrap_or_default()
                .to_owned();
            let (status, body) = if presented == valid && !valid.is_empty() {
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
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}/token")
}

/// The dial seam that mints through the source it was handed, as the
/// production dial does before it connects: the mint is what spends the
/// refresh token, and a rotating server answers it with a rotation the
/// sink must land while the dial still runs.
fn minting_dial(saw: Arc<DialSaw>) -> JmapDial {
    dial_seam(saw, |tokens| async move {
        let _token = tokens.access_token().await?;
        Ok(FakeJmap::provider())
    })
}

#[test]
fn a_rotation_during_the_dial_lands_and_the_persist_keeps_it() {
    let host = Host::new("jmap-add-rotation");
    let endpoint = rotating_token_endpoint("rt-1");

    complete(
        &host,
        grant_document("alice@example.test", &endpoint),
        minting_dial(Arc::new(DialSaw::default())),
    )
    .expect("the sign-in completes over the rotating server");

    let id = jmap_id("alice@example.test");
    // The rotation the mint triggered landed while the dial ran, through
    // the sink, into the entry that stood before the dial began.
    let jmap = stored_jmap(&host, &id);
    assert_eq!(
        jmap.oauth.expect("the grant").refresh_token.expose(),
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
            .jmap
            .expect("the jmap section")
            .oauth
            .expect("the grant")
            .refresh_token
            .expose(),
        "rotated-1",
        "the dial table's entry carries the advanced grant"
    );
}
