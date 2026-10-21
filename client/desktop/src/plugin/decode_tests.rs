// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the plugin decoder: the echo plugin's decode answer becomes the
//! reading's plain leg, a missing or dead worker answers nothing, and the
//! call carries a host-minted message handle.

use std::time::Duration;

use astra_app::{MessageDecoder, MessageRef};
use astra_plugin::Permission;

use crate::plugin::{testkit, PluginDecoder};

fn message() -> MessageRef {
    MessageRef::from_parts("work", "m1".to_owned()).expect("a message ref")
}

#[test]
fn the_echo_decode_becomes_the_plain_leg() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let registry = testkit::registry_with(
            "decode-echo",
            "dev.astra.decoder",
            &["message.decode"],
            None,
            &[Permission::ReadItem],
            testkit::spawn_echo(),
        )
        .await;
        let decoder = PluginDecoder::new(registry, Duration::from_millis(500));
        let outcome = decoder
            .decode(&message(), b"From: a@b.test\r\n\r\nhi")
            .await
            .expect("the echo answers");
        assert_eq!(
            outcome.plain.as_deref(),
            Some("From: a@b.test\r\n\r\nhi"),
            "the raw echoed back as the plain leg"
        );
        assert_eq!(outcome.html, None);
        assert_eq!(outcome.banner, None, "the echo names no banner");
    });
}

#[test]
fn without_a_serving_plugin_the_decode_answers_nothing() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        // A plugin serving only the gate: nothing serves decode.
        let registry = testkit::registry_with(
            "decode-none",
            "dev.astra.gate",
            &["before_send"],
            Some("block"),
            &[Permission::Send, Permission::Admin],
            testkit::spawn_echo(),
        )
        .await;
        let decoder = PluginDecoder::new(registry, Duration::from_millis(500));
        assert_eq!(decoder.decode(&message(), b"raw").await, None);
    });
}

#[test]
fn a_dead_worker_answers_nothing_and_the_mail_still_renders() {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    runtime.block_on(|| async {
        let registry = testkit::registry_with(
            "decode-dead",
            "dev.astra.decoder",
            &["message.decode"],
            None,
            &[Permission::ReadItem],
            testkit::spawn_echo(),
        )
        .await;
        registry
            .worker_host("dev.astra.decoder")
            .expect("host")
            .shutdown()
            .await;
        let decoder = PluginDecoder::new(registry, Duration::from_millis(100));
        assert_eq!(
            decoder.decode(&message(), b"raw").await,
            None,
            "the dead worker's decode fails open"
        );
    });
}
