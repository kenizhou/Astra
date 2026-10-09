// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the detection surface: the bounded run, the typed error paths, the
//! clamped five second tuning, the resolver mapping, and the recommendation view
//! whose literals both sides of the IPC pin.

use std::{future::pending, str::FromStr, time::Duration};

use astra_account::{MissReason, SetupRecommendation};
use astra_autodetect::{
    AuthKind, DetectError, Detected, DetectedDav, DetectedMailSettings, DetectedServer, SocketKind,
    Source, SourceKind,
};
use hickory_resolver::proto::rr::{
    rdata::{MX, SRV},
    Name, RData, Record,
};

use crate::commands::CommandError;
use crate::detection::{
    detect_config, detect_settings_with, mx_record, recommendation_view, srv_record, trim_dns_name,
    DETECT_BOUND,
};

/// A runtime for the bounded-run tests, the same one the host builds.
fn runtime() -> crate::runtime::AppRuntime {
    crate::runtime::build_runtime().expect("the test runtime builds")
}

/// One answer record for the mapping tests, as the resolver hands it back.
fn answer(name: &str, data: RData) -> Record {
    Record::from_rdata(
        Name::from_str(name).expect("the owner name parses"),
        3600,
        data,
    )
}

#[test]
fn an_unknown_domain_yields_the_manual_recommendation_over_the_wire() {
    // What a reserved domain whose lookups all come back clean-empty produces: the
    // manual route with no choices, so the wizard falls back to its manual form.
    let view = detect_settings_with(
        &runtime(),
        "user@nothing-here.invalid",
        DETECT_BOUND,
        || async {
            Ok(Detected::Nothing {
                network_error: false,
            })
        },
    )
    .expect("a clean miss maps");

    assert_eq!(
        view.recommendation,
        SetupRecommendation::Manual {
            reason: MissReason::NothingFound,
        },
    );
    assert!(
        view.choices.is_empty(),
        "the manual route offers no choices: {:?}",
        view.choices,
    );
    assert_eq!(
        serde_json::to_value(&view).expect("the view serialises"),
        serde_json::json!({
            "recommendation": { "Manual": { "reason": "NothingFound" } },
            "choices": [],
            "detected": {
                "outcome": { "nothing": { "network_error": false } },
                "source_kind": null,
            },
        }),
    );
}

#[test]
fn an_offline_miss_carries_the_network_reason() {
    let view = detect_settings_with(&runtime(), "user@offline.example", DETECT_BOUND, || async {
        Ok(Detected::Nothing {
            network_error: true,
        })
    })
    .expect("an offline miss still maps");

    assert_eq!(
        view.recommendation,
        SetupRecommendation::Manual {
            reason: MissReason::NetworkError,
        },
    );
}

#[test]
fn a_detection_error_is_a_typed_error_never_a_panic() {
    let invalid = detect_settings_with(&runtime(), "not an email", DETECT_BOUND, || async {
        Err(DetectError::InvalidEmail)
    })
    .expect_err("an invalid email is an error");
    assert!(
        matches!(invalid, CommandError::Detect(ref message) if message.contains("email")),
        "the error is typed: {invalid:?}",
    );

    let tls = detect_settings_with(&runtime(), "user@example.test", DETECT_BOUND, || async {
        Err(DetectError::Tls("no roots".to_owned()))
    })
    .expect_err("a trust-store failure is an error");
    assert!(
        matches!(tls, CommandError::Detect(ref message) if message.contains("trust")),
        "the error is typed: {tls:?}",
    );
}

#[test]
fn a_run_past_its_bound_is_a_typed_timeout_error() {
    let started = std::time::Instant::now();
    let error = detect_settings_with(
        &runtime(),
        "slow@example.test",
        Duration::from_millis(50),
        pending::<Result<Detected, DetectError>>,
    )
    .expect_err("a hanging run is an error");

    assert!(
        matches!(error, CommandError::Detect(ref message) if message.contains("timed out")),
        "the error is typed: {error:?}",
    );
    assert!(
        started.elapsed() < DETECT_BOUND,
        "the bound fired rather than the caller's patience",
    );
}

#[test]
fn the_detection_tuning_is_clamped_within_five_seconds() {
    let config = detect_config();

    assert_eq!(
        DETECT_BOUND,
        Duration::from_secs(5),
        "the host bound is five seconds",
    );
    assert_eq!(
        config.overall_deadline, DETECT_BOUND,
        "the mail-calendar default of ten seconds is clamped to the host bound",
    );
    assert!(config.http_timeout <= DETECT_BOUND);
    assert!(config.dns_timeout <= DETECT_BOUND);
}

#[test]
fn a_found_imap_recommendation_crosses_with_its_choices() {
    let detected = Detected::Mail(DetectedMailSettings {
        incoming: vec![DetectedServer {
            hostname: "imap.example.test".to_owned(),
            port: 993,
            socket: SocketKind::Tls,
            auth: vec![AuthKind::PasswordCleartext],
            username: "user@example.test".to_owned(),
        }],
        outgoing: Vec::new(),
        is_trusted: true,
        source: Source {
            kind: SourceKind::Autoconfig,
            url: "https://autoconfig.example.test/mail/config-v1.1.xml".to_owned(),
        },
        oauth_issuer: None,
        dav: DetectedDav::default(),
    });

    let view = recommendation_view("user@example.test", detected);

    assert_eq!(
        serde_json::to_value(&view).expect("the view serialises"),
        serde_json::json!({
            "recommendation": {
                "Imap": {
                    "email": "user@example.test",
                    "imap_host": "imap.example.test",
                    "imap_security": "implicit_tls",
                    "smtp_security": "implicit_tls",
                    "incoming": {
                        "protocol": "IMAP",
                        "hostname": "imap.example.test",
                        "port": 993,
                        "security": "SSL/TLS",
                        "username": "user@example.test",
                    },
                    "is_trusted": true,
                    "source": "https://autoconfig.example.test/mail/config-v1.1.xml",
                },
            },
            "choices": [
                { "capability": "Mail", "on": true, "server_found": true },
                { "capability": "Calendar", "on": false, "server_found": false },
                { "capability": "Contacts", "on": false, "server_found": false },
            ],
            "detected": { "outcome": "mail", "source_kind": "Autoconfig" },
        }),
    );
}

#[test]
fn mx_answers_map_onto_the_detection_records() {
    let wire = answer(
        "example.test.",
        RData::MX(MX::new(
            10,
            Name::from_str("mail.example.test.").expect("the exchange parses"),
        )),
    );

    let mapped = mx_record(&wire).expect("the MX answer maps");
    assert_eq!(
        (mapped.preference, mapped.exchange.as_str()),
        (10, "mail.example.test")
    );
}

#[test]
fn srv_answers_map_onto_the_detection_records() {
    let wire = answer(
        "_imaps._tcp.example.test.",
        RData::SRV(SRV::new(
            1,
            0,
            993,
            Name::from_str("imap.example.test.").expect("the target parses"),
        )),
    );

    let mapped = srv_record(&wire).expect("the SRV answer maps");
    assert_eq!(
        (
            mapped.priority,
            mapped.weight,
            mapped.port,
            mapped.target.as_str()
        ),
        (1, 0, 993, "imap.example.test")
    );
}

#[test]
fn an_answer_of_another_type_maps_to_no_record() {
    let wire = answer(
        "example.test.",
        RData::MX(MX::new(0, Name::from_str(".").expect("the root parses"))),
    );
    let srv = answer(
        "_imaps._tcp.example.test.",
        RData::SRV(SRV::new(
            0,
            0,
            0,
            Name::from_str(".").expect("the root parses"),
        )),
    );

    assert!(
        srv_record(&wire).is_none(),
        "an SRV slot takes only SRV data"
    );
    assert!(mx_record(&srv).is_none(), "an MX slot takes only MX data");
}

#[test]
fn dns_targets_lose_only_the_wire_trailing_dot() {
    assert_eq!(trim_dns_name("mail.example.test."), "mail.example.test");
    assert_eq!(trim_dns_name("."), ".");
    assert_eq!(trim_dns_name("mail.example.test"), "mail.example.test");
}
