// SPDX-License-Identifier: GPL-3.0-only
//! The key verbs' review tests: the promote's load-bearing trust pairing
//! (the app orchestration's CLI copy), the reject's removal plus rejected
//! decision, the previews that write nothing, the read verbs' preview
//! refusal, the FK-unregistered error, and the audit row every invocation
//! lands. The fixture helpers live in `keys_tests`.

use super::fakes::{temp_dir, write_index};
use super::keys_tests::{armored, data, decision, fp, gen, line, seeded, stage, PEER};
use super::run;

#[test]
fn keys_promote_lands_the_key_and_the_trust_decision() {
    let dir = seeded();
    let cert = gen(&format!("Peer <{PEER}>"));
    stage(&dir, PEER, &cert);
    let answer = data(&run(
        &line(&[
            "keys",
            "promote",
            "--account",
            "alpha",
            "--standard",
            "openpgp",
            "--fingerprint",
            &fp(&cert),
        ]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(answer["state"], "promoted");
    assert_eq!(answer["key"]["fingerprint"], fp(&cert));
    assert_eq!(answer["key"]["origin"], "autocrypt");
    assert_eq!(answer["key"]["has_private"], false);
    // The load-bearing pairing: the acceptance binds the staged row's peer
    // email (the fallback read before the promote drains staging).
    assert_eq!(
        decision(&dir, PEER, &fp(&cert)).as_deref(),
        Some("unverified")
    );
    // The staging drains and the keystore holds the row.
    let collected = data(&run(
        &line(&["keys", "collected", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(collected["keys"], serde_json::json!([]));
    let listed = data(&run(
        &line(&["keys", "list", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(listed["keys"].as_array().unwrap().len(), 1);
}

#[test]
fn a_verified_promote_upgrades_and_an_unverified_one_never_downgrades() {
    let dir = seeded();
    let cert = gen(&format!("Peer <{PEER}>"));
    stage(&dir, PEER, &cert);
    let promote = |verified: bool| {
        let mut words = line(&[
            "keys",
            "promote",
            "--account",
            "alpha",
            "--standard",
            "openpgp",
            "--fingerprint",
            &fp(&cert),
        ]);
        if verified {
            words.push("--verified".to_owned());
        }
        run(&words, &dir, &mut std::io::empty())
    };
    assert_eq!(promote(true).code.exit(), 0);
    assert_eq!(
        decision(&dir, PEER, &fp(&cert)).as_deref(),
        Some("verified")
    );
    // A re-staged copy accepted without verification keeps the standing
    // verified decision (the keystore row wins untouched).
    stage(&dir, PEER, &cert);
    assert_eq!(promote(false).code.exit(), 0);
    assert_eq!(
        decision(&dir, PEER, &fp(&cert)).as_deref(),
        Some("verified")
    );
}

#[test]
fn keys_promote_binds_the_given_email_over_the_staged_one() {
    let dir = seeded();
    let cert = gen(&format!("Peer <{PEER}>"));
    stage(&dir, "staged@elsewhere.local", &cert);
    let answer = data(&run(
        &line(&[
            "keys",
            "promote",
            "--account",
            "alpha",
            "--standard",
            "openpgp",
            "--fingerprint",
            &fp(&cert),
            "--email",
            PEER,
        ]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(answer["key"]["email"], PEER);
    assert_eq!(
        decision(&dir, PEER, &fp(&cert)).as_deref(),
        Some("unverified")
    );
    assert_eq!(decision(&dir, "staged@elsewhere.local", &fp(&cert)), None);
}

#[test]
fn keys_reject_removes_the_staging_row_and_records_rejected() {
    let dir = seeded();
    let cert = gen(&format!("Peer <{PEER}>"));
    let id = stage(&dir, PEER, &cert);
    let answer = data(&run(
        &line(&[
            "keys",
            "reject",
            "--account",
            "alpha",
            "--id",
            &id.to_string(),
            "--standard",
            "openpgp",
            "--fingerprint",
            &fp(&cert),
        ]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(answer["state"], "rejected");
    assert_eq!(answer["id"], id);
    assert_eq!(
        decision(&dir, PEER, &fp(&cert)).as_deref(),
        Some("rejected")
    );
    let collected = data(&run(
        &line(&["keys", "collected", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(collected["keys"], serde_json::json!([]));
    // The rejection stands: a re-staged copy promoted without verification
    // never downgrades it.
    stage(&dir, PEER, &cert);
    run(
        &line(&[
            "keys",
            "promote",
            "--account",
            "alpha",
            "--standard",
            "openpgp",
            "--fingerprint",
            &fp(&cert),
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(
        decision(&dir, PEER, &fp(&cert)).as_deref(),
        Some("rejected")
    );
}

#[test]
fn the_key_previews_name_the_action_and_write_nothing() {
    let dir = seeded();
    let cert = gen(&format!("Peer <{PEER}>"));
    let id = stage(&dir, PEER, &cert);

    let import = data(&run(
        &line(&[
            "keys",
            "import",
            "--account",
            "alpha",
            "--plain",
            &armored(&cert),
            "--preview",
        ]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(import["action"], "import_key");
    assert_eq!(import["account"], "alpha");
    assert!(import["bytes"].as_u64().unwrap() > 0);

    let promote = data(&run(
        &line(&[
            "keys",
            "promote",
            "--account",
            "alpha",
            "--standard",
            "openpgp",
            "--fingerprint",
            &fp(&cert),
            "--preview",
        ]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(promote["action"], "promote_key");
    assert_eq!(promote["fingerprint"], fp(&cert));

    let reject = data(&run(
        &line(&[
            "keys",
            "reject",
            "--account",
            "alpha",
            "--id",
            &id.to_string(),
            "--standard",
            "openpgp",
            "--fingerprint",
            &fp(&cert),
            "--preview",
        ]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(reject["action"], "reject_key");
    assert_eq!(reject["id"], id);

    // Nothing moved: no keystore row, the staging row stands, no decision.
    let listed = data(&run(
        &line(&["keys", "list", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(listed["keys"], serde_json::json!([]));
    let collected = data(&run(
        &line(&["keys", "collected", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(collected["keys"].as_array().unwrap().len(), 1);
    assert_eq!(decision(&dir, PEER, &fp(&cert)), None);
}

#[test]
fn a_read_verb_refuses_preview_and_the_mutating_flags_are_checked() {
    let dir = seeded();
    for sub in ["list", "collected"] {
        let outcome = run(
            &line(&["keys", sub, "--account", "alpha", "--preview"]),
            &dir,
            &mut std::io::empty(),
        );
        assert_eq!(outcome.code.exit(), 2, "keys {sub} is a read");
        let text = outcome.envelope.render(false).unwrap();
        assert!(
            text.contains("only the mutating verbs ship --preview"),
            "{text}"
        );
    }
    let bad_id = run(
        &line(&[
            "keys",
            "reject",
            "--account",
            "alpha",
            "--id",
            "seven",
            "--standard",
            "openpgp",
            "--fingerprint",
            "AA",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(bad_id.code.exit(), 2);
    let text = bad_id.envelope.render(false).unwrap();
    assert!(text.contains("--id"), "{text}");

    let bad_standard = run(
        &line(&[
            "keys",
            "promote",
            "--account",
            "alpha",
            "--standard",
            "pgp",
            "--fingerprint",
            "AA",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(bad_standard.code.exit(), 2);
}

#[test]
fn an_unregistered_account_fails_import_and_promote_with_the_pointer() {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    // No crypto.sqlite account row: the app never registered this account.
    let cert = gen(&format!("Peer <{PEER}>"));
    let import = run(
        &line(&[
            "keys",
            "import",
            "--account",
            "alpha",
            "--plain",
            &armored(&cert),
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(import.code.exit(), 1);
    let text = import.envelope.render(false).unwrap();
    assert!(text.contains("Astra Mail"), "{text}");

    stage(&dir, PEER, &cert);
    let promote = run(
        &line(&[
            "keys",
            "promote",
            "--account",
            "alpha",
            "--standard",
            "openpgp",
            "--fingerprint",
            &fp(&cert),
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(promote.code.exit(), 1);
    let text = promote.envelope.render(false).unwrap();
    assert!(text.contains("Astra Mail"), "{text}");

    // The reads carry no foreign key: they answer regardless.
    assert_eq!(
        run(
            &line(&["keys", "list", "--account", "alpha"]),
            &dir,
            &mut std::io::empty()
        )
        .code
        .exit(),
        0
    );
    assert_eq!(
        run(
            &line(&["keys", "collected", "--account", "alpha"]),
            &dir,
            &mut std::io::empty()
        )
        .code
        .exit(),
        0
    );
}

#[test]
fn every_keys_invocation_lands_its_audit_row() {
    let dir = seeded();
    let cert = gen(&format!("Peer <{PEER}>"));
    let id = stage(&dir, PEER, &cert);
    let bytes = armored(&cert).len();
    let lines = [
        (
            line(&["keys", "list", "--account", "alpha"]),
            "keys list",
            "account=alpha".to_owned(),
            "ok".to_owned(),
        ),
        (
            line(&["keys", "collected", "--account", "alpha"]),
            "keys collected",
            "account=alpha".to_owned(),
            "ok".to_owned(),
        ),
        (
            line(&[
                "keys",
                "import",
                "--account",
                "alpha",
                "--plain",
                &armored(&cert),
                "--preview",
            ]),
            "keys import",
            format!("account=alpha bytes={bytes} source=arg"),
            "preview".to_owned(),
        ),
        (
            line(&[
                "keys",
                "promote",
                "--account",
                "alpha",
                "--standard",
                "openpgp",
                "--fingerprint",
                &fp(&cert),
                "--verified",
            ]),
            "keys promote",
            format!(
                "account=alpha standard=openpgp fingerprint={} verified=true",
                fp(&cert)
            ),
            "ok".to_owned(),
        ),
        (
            line(&[
                "keys",
                "promote",
                "--account",
                "alpha",
                "--standard",
                "openpgp",
                "--fingerprint",
                &fp(&cert),
                "--email",
                PEER,
            ]),
            "keys promote",
            format!(
                "account=alpha standard=openpgp fingerprint={} email=1 verified=false",
                fp(&cert)
            ),
            "ok".to_owned(),
        ),
        (
            line(&[
                "keys",
                "reject",
                "--account",
                "alpha",
                "--id",
                &id.to_string(),
                "--standard",
                "openpgp",
                "--fingerprint",
                &fp(&cert),
            ]),
            "keys reject",
            format!(
                "account=alpha id={id} standard=openpgp fingerprint={}",
                fp(&cert)
            ),
            "ok".to_owned(),
        ),
        (
            line(&["keys", "promote", "--account", "alpha"]),
            "keys promote",
            String::new(),
            "usage".to_owned(),
        ),
    ];
    for (at, (words, verb, preview, outcome)) in lines.into_iter().enumerate() {
        let correlation = format!("turn-{at}-{}", verb.replace(' ', "-"));
        let mut words = words;
        words.push("--correlation-id".to_owned());
        words.push(correlation.clone());
        run(&words, &dir, &mut std::io::empty());
        let conn = rusqlite::Connection::open(dir.join(super::AI_STORE_FILE)).unwrap();
        let row: (String, String, String) = conn
            .query_row(
                "SELECT verb, arg_preview, outcome FROM audit_log
                 WHERE correlation_id = ?1",
                [&correlation],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (verb.to_owned(), preview, outcome));
    }
}
