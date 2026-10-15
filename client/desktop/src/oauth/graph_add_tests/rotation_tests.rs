// SPDX-License-Identifier: GPL-3.0-only
//! The pre-register ordering's proof: a rotation that happens during the
//! dial lands in the sink, and the grant the completion persists is the
//! advanced one, never the copy parsed before the dial. Its own file for
//! the mock rotating token endpoint, which is the piece that makes the
//! ordering observable offline.

use std::io::{Read, Write};

use astra_oauth::OAuthClient;

use astra_account::GraphTokenSource;

use super::*;

fn rotating_token_endpoint(initial: &str) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let mut valid = initial.to_owned();
    let mut generation = 0usize;
    std::thread::spawn(move || {
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
                let Some((head, body)) = text.split_once("\r\n\r\n") else {
                    continue;
                };
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
            let request = String::from_utf8_lossy(&raw).into_owned();
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
                let body = format!(
                    r#"{{"token_type":"Bearer","expires_in":3600,"access_token":"AT-{generation}","refresh_token":"{valid}"}}"#
                );
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
            } else {
                let body = r#"{"error":"invalid_grant","error_description":"replay"}"#;
                let resp = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        }
    });
    format!("http://{addr}/token")
}

/// The source seam over the mock rotating endpoint, so a forced refresh
/// during the dial rotates through the real sink wiring.
fn rotating_source_seam(endpoint: String) -> SourceBuilder {
    Arc::new(move |provider, account, refresh_token, sink| {
        let mut mock = provider.clone();
        mock.token_endpoint = endpoint.clone();
        let client = OAuthClient::new(mock).map_err(|err| err.to_string())?;
        Ok(GraphTokenSource::from_parts(
            client,
            account,
            refresh_token,
            sink,
            "graph",
            astra_account::CredentialOrigin::FreshSignIn,
        ))
    })
}

#[test]
fn a_rotation_during_the_dial_lands_in_the_sink_and_persists_the_advanced_grant() {
    let host = Host::new("graph-add-rotation");
    let endpoint = rotating_token_endpoint("fresh-refresh");
    // The dial forces one refresh of the seeded token, which the mock
    // answers with a rotation; the sink must be able to land it, which is
    // the whole point of registering before dialing.
    let dial = dial_seam(Arc::new(DialSaw::default()), |tokens| async move {
        tokens.access_token_replacing("fresh-access").await?;
        Ok(astra_account::MicrosoftProviders {
            mail: vec![FakeGraph::mail()],
            calendar: None,
            calendar_error: Some("calendar access denied: 403".to_owned()),
            calendar_reauth_required: true,
        })
    });

    let added = complete(
        &host,
        lookup_seam("alice@example.test"),
        dial,
        rotating_source_seam(endpoint),
    )
    .expect("the sign-in completes over the rotating server");

    assert!(
        added.calendar_withheld,
        "the withheld calendar carries to the surface"
    );
    let id = graph_id("alice@example.test");
    let persists = stored_documents(&host, &id);
    assert_eq!(
        persists.len(),
        2,
        "the during-dial rotation persisted once and the completion once"
    );
    for document in &persists {
        let graph = astra_account::load_str(document)
            .expect("each stored document loads")
            .graph
            .expect("each document is a graph one");
        assert_eq!(
            graph.refresh_token.expose(),
            "rotated-1",
            "every persisted document carries the advanced grant, never the superseded one"
        );
    }
    let entry = host.dials.of(&id).expect("the entry stands").0;
    assert_eq!(
        entry.graph.expect("the grant").refresh_token.expose(),
        "rotated-1",
        "the entry the next dial reads is the advanced one"
    );
}
