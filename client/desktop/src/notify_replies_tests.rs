// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the reply notification bridge: the enabled gate, the absence of
//! focus suppression (the reminder precedent), the fixed "Reply received"
//! title, the sender/subject body, and the reply history event, all over a
//! recording sink so no OS is involved.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use astra_app::{NewMailBatch, NewMailNotice};
use engine_api::{AccountId, EmailAddress, ProviderKey};

use super::{HistoryEmitter, NotifyBridge, OsNotifier, REPLY_EVENT};

/// Every notification the sink was asked to show.
#[derive(Default)]
struct Recording {
    shown: Mutex<Vec<(String, String)>>,
}

/// The sink the bridge holds a second handle to, so the test reads what
/// crossed.
#[derive(Clone, Default)]
struct SharedRecording(Arc<Recording>);

impl OsNotifier for SharedRecording {
    fn show(&self, title: &str, body: &str) {
        self.0
            .shown
            .lock()
            .unwrap()
            .push((title.to_owned(), body.to_owned()));
    }
}

/// A bridge over a recording sink, its focus state held by the test.
fn bridge(focused: bool) -> (NotifyBridge, Arc<AtomicBool>, Arc<Recording>) {
    let focus = Arc::new(AtomicBool::new(focused));
    let probe: Arc<dyn Fn() -> bool + Send + Sync> = {
        let focus = Arc::clone(&focus);
        Arc::new(move || focus.load(Ordering::SeqCst))
    };
    let sink = SharedRecording::default();
    let record = Arc::clone(&sink.0);
    (NotifyBridge::new(Box::new(sink), probe), focus, record)
}

/// One notice, minted around `n`.
fn notice(n: usize) -> NewMailNotice {
    NewMailNotice {
        account: AccountId::try_from("work").unwrap(),
        key: ProviderKey::new(format!("k{n}")).unwrap(),
        from: Some(EmailAddress::named(
            format!("Sender {n}"),
            format!("sender{n}@example.test"),
        )),
        subject: Some(format!("Subject {n}")),
        preview: Some(format!("Preview {n}")),
    }
}

/// A batch of `count` messages, no overflow.
fn batch(count: usize) -> NewMailBatch {
    NewMailBatch {
        messages: (0..count).map(notice).collect(),
        overflow: 0,
    }
}

/// What the sink has shown so far.
fn shown(record: &Arc<Recording>) -> Vec<(String, String)> {
    record.shown.lock().unwrap().clone()
}

/// Every batch the history channel was asked to emit, and whether it refuses.
#[derive(Default)]
struct Emitted {
    batches: Mutex<Vec<NewMailBatch>>,
    refused: AtomicBool,
}

/// The history channel the bridge holds a second handle to.
#[derive(Clone, Default)]
struct SharedEmitted(Arc<Emitted>);

impl HistoryEmitter for SharedEmitted {
    fn emit(&self, batch: &NewMailBatch) -> Result<(), String> {
        if self.0.refused.load(Ordering::SeqCst) {
            return Err("the channel is closed".to_owned());
        }
        self.0.batches.lock().unwrap().push(batch.clone());
        Ok(())
    }
}

/// A recording history channel and the handle the test reads it through.
fn emitter() -> (SharedEmitted, Arc<Emitted>) {
    let sink = SharedEmitted::default();
    let record = Arc::clone(&sink.0);
    (sink, record)
}

/// What the history channel has carried so far.
fn emitted(record: &Arc<Emitted>) -> Vec<NewMailBatch> {
    record.batches.lock().unwrap().clone()
}

#[test]
fn a_reply_delivers_one_notification_per_message_titled_reply_received() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver_replies(&batch(2), true);
    let shown = shown(&record);
    assert_eq!(shown.len(), 2, "one per reply: {shown:?}");
    assert_eq!(
        shown[0],
        (
            "Reply received".to_owned(),
            "Sender 0\nSubject 0".to_owned()
        ),
        "the title is the fixed word, the body the sender and subject"
    );
}

#[test]
fn a_reply_fires_even_while_the_window_is_focused() {
    // The no-focus-suppression policy: an explicit user request fires while
    // the window is focused; the enabled switch is the only gate.
    let (bridge, _focus, record) = bridge(true);
    bridge.deliver_replies(&batch(2), true);
    assert_eq!(
        shown(&record).len(),
        2,
        "a focused window does not suppress a reply notice"
    );
}

#[test]
fn nothing_replies_while_the_switch_is_off() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver_replies(&batch(2), false);
    assert!(
        shown(&record).is_empty(),
        "the kill switch is the reply's only gate"
    );
}

#[test]
fn nothing_replies_for_an_empty_batch() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver_replies(&NewMailBatch::default(), true);
    assert_eq!(
        shown(&record),
        [] as [(std::string::String, std::string::String); 0]
    );
}

#[test]
fn a_reply_without_a_sender_or_subject_still_announces_itself() {
    let (bridge, _focus, record) = bridge(false);
    let bare = NewMailNotice {
        account: AccountId::try_from("work").unwrap(),
        key: ProviderKey::new("k").unwrap(),
        from: None,
        subject: None,
        preview: None,
    };
    bridge.deliver_replies(
        &NewMailBatch {
            messages: vec![bare],
            overflow: 0,
        },
        true,
    );
    let shown = shown(&record);
    assert_eq!(shown.len(), 1);
    assert_eq!(
        shown[0],
        ("Reply received".to_owned(), "You have a reply".to_owned())
    );
}

#[test]
fn the_reply_batch_crosses_on_the_reply_event() {
    assert_eq!(REPLY_EVENT, "reply", "the web listens on this name");
    let (bridge, _focus, record) = bridge(false);
    let (sink, channel) = emitter();
    let bridge = bridge.with_reply_history(Box::new(sink));
    let landed = batch(2);
    bridge.deliver_replies(&landed, true);
    assert_eq!(
        emitted(&channel),
        vec![landed],
        "the whole reply batch crosses, overflow zero"
    );
    assert_eq!(shown(&record).len(), 2, "the OS notifications still cross");
}

#[test]
fn nothing_replies_across_while_the_switch_is_off() {
    let (bridge, _focus, _record) = bridge(false);
    let (sink, channel) = emitter();
    let bridge = bridge.with_reply_history(Box::new(sink));
    bridge.deliver_replies(&batch(2), false);
    assert!(
        emitted(&channel).is_empty(),
        "the kill switch beats the reply history"
    );
}

#[test]
fn a_reply_history_failure_never_suppresses_the_os_notification() {
    let (bridge, _focus, record) = bridge(false);
    let (sink, channel) = emitter();
    channel.refused.store(true, Ordering::SeqCst);
    let bridge = bridge.with_reply_history(Box::new(sink));
    bridge.deliver_replies(&batch(2), true);
    assert_eq!(
        shown(&record).len(),
        2,
        "the OS notification crosses whatever the history channel answers"
    );
    assert_eq!(emitted(&channel), [] as [NewMailBatch; 0]);
}

#[test]
fn the_reply_channel_is_distinct_from_the_new_mail_and_reminder_channels() {
    let (bridge, _focus, _record) = bridge(false);
    let (new_sink, new_channel) = emitter();
    let (rem_sink, rem_channel) = emitter();
    let (reply_sink, reply_channel) = emitter();
    let bridge = bridge
        .with_history(Box::new(new_sink))
        .with_reminder_history(Box::new(rem_sink))
        .with_reply_history(Box::new(reply_sink));
    let landed = batch(1);
    bridge.deliver_replies(&landed, true);
    assert_eq!(emitted(&new_channel), [] as [NewMailBatch; 0]);
    assert_eq!(emitted(&rem_channel), [] as [NewMailBatch; 0]);
    assert_eq!(emitted(&reply_channel), vec![landed]);
}
