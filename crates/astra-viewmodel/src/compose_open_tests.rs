// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the compose-open view-model: the exact JSON wire shape a mailto
//! open crosses the IPC in, and that an emitter's payload with the address
//! and the five fields alone round trips.

use super::MailtoOpen;

#[test]
fn a_mailto_open_crosses_with_the_account_and_the_five_fields() {
    let open = MailtoOpen {
        from: engine_api::AccountId::try_from("work").unwrap(),
        to: "ada@example.test".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: "Lunch".to_owned(),
        body: "Noon?".to_owned(),
    };

    let json = serde_json::to_string(&open).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"from":"work","to":"ada@example.test","cc":"","bcc":"","#,
            r#""subject":"Lunch","body":"Noon?"}"#
        )
    );

    let back: MailtoOpen = serde_json::from_str(&json).unwrap();
    assert_eq!(back, open);
}
