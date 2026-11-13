// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the notification bridge: the focus rules, the
//! three-then-summary bound, the kill switch, and the shape of one
//! message's notification, all over a recording sink so no OS is involved.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use astra_app::{NewMailBatch, NewMailNotice};
use engine_api::{AccountId, EmailAddress, ProviderKey};

use super::{HistoryEmitter, NotifyBridge, OsNotifier, NEW_MAIL_EVENT, REMINDER_EVENT};

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

#[test]
fn one_notification_per_message_up_to_three_while_unfocused() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver(&batch(3), true);
    let shown = shown(&record);
    assert_eq!(shown.len(), 3, "one per message: {shown:?}");
    assert_eq!(
        shown[0],
        ("Sender 0".to_owned(), "Subject 0\nPreview 0".to_owned())
    );
}

#[test]
fn a_single_summary_replaces_the_notifications_past_three() {
    let (bridge, _focus, record) = bridge(false);
    let mut folded = batch(3);
    folded.overflow = 2;
    bridge.deliver(&folded, true);
    let shown = shown(&record);
    assert_eq!(shown.len(), 1, "one summary, not a storm: {shown:?}");
    assert_eq!(
        shown[0],
        ("New mail".to_owned(), "5 new messages".to_owned()),
        "the summary counts the overflow the queue folded away"
    );
}

#[test]
fn nothing_while_the_window_is_focused() {
    // The person is already looking at the list; a notification would be
    // noise beside it.
    let (bridge, focus, record) = bridge(true);
    bridge.deliver(&batch(2), true);
    assert_eq!(
        shown(&record),
        [] as [(std::string::String, std::string::String); 0]
    );
    focus.store(false, Ordering::SeqCst);
    bridge.deliver(&batch(2), true);
    assert_eq!(shown(&record).len(), 2, "the next unfocused pass announces");
}

#[test]
fn nothing_while_the_switch_is_off() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver(&batch(2), false);
    assert!(
        shown(&record).is_empty(),
        "the kill switch beats every other rule"
    );
}

#[test]
fn nothing_for_an_empty_batch() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver(&NewMailBatch::default(), true);
    assert_eq!(
        shown(&record),
        [] as [(std::string::String, std::string::String); 0]
    );
}

#[test]
fn a_message_without_a_sender_or_subject_still_announces_itself() {
    // What the envelope omitted the notification falls back past: an
    // address-less From, no subject, no preview, and the notification is
    // still worth raising.
    let (bridge, _focus, record) = bridge(false);
    let bare = NewMailNotice {
        account: AccountId::try_from("work").unwrap(),
        key: ProviderKey::new("k").unwrap(),
        from: None,
        subject: None,
        preview: None,
    };
    bridge.deliver(
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
        ("New mail".to_owned(), "You have a new message".to_owned())
    );
}

/// Every batch the history channel was asked to emit, and whether the
/// channel refuses its crossings.
#[derive(Default)]
struct Emitted {
    batches: Mutex<Vec<NewMailBatch>>,
    refused: AtomicBool,
}

/// The history channel the bridge holds a second handle to, so the test
/// reads what crossed.
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
fn the_notified_batch_crosses_on_the_new_mail_event() {
    assert_eq!(NEW_MAIL_EVENT, "new-mail", "the web listens on this name");
    let (bridge, _focus, record) = bridge(false);
    let (sink, channel) = emitter();
    let bridge = bridge.with_history(Box::new(sink));
    let mut landed = batch(2);
    landed.overflow = 1;
    bridge.deliver(&landed, true);
    assert_eq!(
        emitted(&channel),
        vec![landed],
        "the whole drained batch crosses, overflow included"
    );
    assert_eq!(shown(&record).len(), 2, "the OS notifications still cross");
}

#[test]
fn the_whole_batch_crosses_even_when_the_os_shows_a_summary() {
    let (bridge, _focus, record) = bridge(false);
    let (sink, channel) = emitter();
    let bridge = bridge.with_history(Box::new(sink));
    let mut landed = batch(3);
    landed.overflow = 2;
    bridge.deliver(&landed, true);
    assert_eq!(shown(&record).len(), 1, "one summary, not a storm");
    assert_eq!(
        emitted(&channel),
        vec![landed],
        "the history records the messages the summary folded"
    );
}

#[test]
fn nothing_crosses_while_the_switch_is_off() {
    let (bridge, _focus, _record) = bridge(false);
    let (sink, channel) = emitter();
    let bridge = bridge.with_history(Box::new(sink));
    bridge.deliver(&batch(2), false);
    assert!(
        emitted(&channel).is_empty(),
        "the kill switch beats the history"
    );
}

#[test]
fn nothing_crosses_while_the_window_is_focused() {
    let (bridge, _focus, _record) = bridge(true);
    let (sink, channel) = emitter();
    let bridge = bridge.with_history(Box::new(sink));
    bridge.deliver(&batch(2), true);
    assert!(
        emitted(&channel).is_empty(),
        "a focused window records nothing: the history is of notified mail"
    );
}

#[test]
fn nothing_crosses_for_an_empty_batch() {
    let (bridge, _focus, _record) = bridge(false);
    let (sink, channel) = emitter();
    let bridge = bridge.with_history(Box::new(sink));
    bridge.deliver(&NewMailBatch::default(), true);
    assert_eq!(emitted(&channel), [] as [NewMailBatch; 0]);
}

#[test]
fn a_history_failure_never_suppresses_the_os_notification() {
    let (bridge, _focus, record) = bridge(false);
    let (sink, channel) = emitter();
    channel.refused.store(true, Ordering::SeqCst);
    let bridge = bridge.with_history(Box::new(sink));
    bridge.deliver(&batch(2), true);
    assert_eq!(
        shown(&record).len(),
        2,
        "the OS notification crosses whatever the history channel answers"
    );
    assert_eq!(emitted(&channel), [] as [NewMailBatch; 0]);
}

#[test]
fn the_headless_bridge_logs_the_notification_and_carries_no_history() {
    // A subject unique to this test, so the shared log buffer's concurrent
    // lines never answer for it: the headless sink logs the notification
    // and nothing else, the history channel being absent by construction.
    let probe = NewMailNotice {
        account: AccountId::try_from("work").unwrap(),
        key: ProviderKey::new("headless-probe").unwrap(),
        from: Some(EmailAddress::named("Headless Probe", "probe@example.test")),
        subject: Some("headless-history-probe".to_owned()),
        preview: None,
    };
    let lines = crate::log_capture::captured();
    let before = lines.lock().unwrap().len();
    let bridge = NotifyBridge::logged();
    bridge.deliver(
        &NewMailBatch {
            messages: vec![probe],
            overflow: 0,
        },
        true,
    );
    let slice = lines.lock().unwrap()[before..].to_vec();
    let own: Vec<&(log::Level, String)> = slice
        .iter()
        .filter(|(_, line)| line.contains("headless-history-probe"))
        .collect();
    assert_eq!(
        own.len(),
        1,
        "the headless path logs exactly its one notification for the batch, \
         no history crossing beside it (the channel is absent by construction): {own:?}"
    );
    assert!(
        own[0]
            .1
            .contains("notification: Headless Probe: headless-history-probe"),
        "the headless sink logs what would have crossed: {own:?}"
    );
}

#[test]
fn the_event_payload_serialises_the_batch_snake_case() {
    let mut landed = batch(1);
    landed.overflow = 2;
    let json = serde_json::to_value(&landed).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "messages": [{
                "account": "work",
                "key": "k0",
                "from": { "name": "Sender 0", "email": "sender0@example.test" },
                "subject": "Subject 0",
                "preview": "Preview 0"
            }],
            "overflow": 2
        })
    );
}

#[test]
fn a_bare_notice_serialises_its_absent_fields_as_null() {
    let bare = NewMailNotice {
        account: AccountId::try_from("work").unwrap(),
        key: ProviderKey::new("k").unwrap(),
        from: None,
        subject: None,
        preview: None,
    };
    let json = serde_json::to_value(&bare).unwrap();
    assert_eq!(json["from"], serde_json::Value::Null);
    assert_eq!(json["subject"], serde_json::Value::Null);
    assert_eq!(json["preview"], serde_json::Value::Null);
    assert_eq!(json["account"], "work");
    assert_eq!(json["key"], "k");
}

#[test]
fn a_reminder_delivers_one_notification_per_message_while_unfocused() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver_reminders(&batch(2), true);
    let shown = shown(&record);
    assert_eq!(shown.len(), 2, "one per reminder: {shown:?}");
    assert_eq!(
        shown[0],
        ("Reminder".to_owned(), "Subject 0".to_owned()),
        "the title is the fixed word, the body the captured subject"
    );
}

#[test]
fn a_reminder_fires_even_while_the_window_is_focused() {
    // The no-focus-suppression policy: an explicit user request fires while
    // the window is focused; the enabled switch is the only gate.
    let (bridge, _focus, record) = bridge(true);
    bridge.deliver_reminders(&batch(2), true);
    assert_eq!(
        shown(&record).len(),
        2,
        "a focused window does not suppress a reminder"
    );
}

#[test]
fn nothing_reminds_while_the_switch_is_off() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver_reminders(&batch(2), false);
    assert!(
        shown(&record).is_empty(),
        "the kill switch is the reminder's only gate"
    );
}

#[test]
fn nothing_reminds_for_an_empty_batch() {
    let (bridge, _focus, record) = bridge(false);
    bridge.deliver_reminders(&NewMailBatch::default(), true);
    assert_eq!(
        shown(&record),
        [] as [(std::string::String, std::string::String); 0]
    );
}

#[test]
fn a_reminder_without_a_subject_still_announces_itself() {
    let (bridge, _focus, record) = bridge(false);
    let bare = NewMailNotice {
        account: AccountId::try_from("work").unwrap(),
        key: ProviderKey::new("k").unwrap(),
        from: None,
        subject: None,
        preview: None,
    };
    bridge.deliver_reminders(
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
        ("Reminder".to_owned(), "You have a reminder".to_owned())
    );
}

#[test]
fn the_reminder_batch_crosses_on_the_reminder_event() {
    assert_eq!(REMINDER_EVENT, "reminder", "the web listens on this name");
    let (bridge, _focus, record) = bridge(false);
    let (sink, channel) = emitter();
    let bridge = bridge.with_reminder_history(Box::new(sink));
    let landed = batch(2);
    bridge.deliver_reminders(&landed, true);
    assert_eq!(
        emitted(&channel),
        vec![landed],
        "the whole woken batch crosses, overflow zero"
    );
    assert_eq!(shown(&record).len(), 2, "the OS notifications still cross");
}

#[test]
fn nothing_reminds_across_while_the_switch_is_off() {
    let (bridge, _focus, _record) = bridge(false);
    let (sink, channel) = emitter();
    let bridge = bridge.with_reminder_history(Box::new(sink));
    bridge.deliver_reminders(&batch(2), false);
    assert!(
        emitted(&channel).is_empty(),
        "the kill switch beats the reminder history"
    );
}

#[test]
fn a_reminder_history_failure_never_suppresses_the_os_notification() {
    let (bridge, _focus, record) = bridge(false);
    let (sink, channel) = emitter();
    channel.refused.store(true, Ordering::SeqCst);
    let bridge = bridge.with_reminder_history(Box::new(sink));
    bridge.deliver_reminders(&batch(2), true);
    assert_eq!(
        shown(&record).len(),
        2,
        "the OS notification crosses whatever the history channel answers"
    );
    assert_eq!(emitted(&channel), [] as [NewMailBatch; 0]);
}

#[test]
fn the_reminder_channel_is_distinct_from_the_new_mail_channel() {
    // A reminder crosses on the reminder channel only, never the new-mail
    // one: the two histories record their own landings.
    let (bridge, _focus, _record) = bridge(false);
    let (new_sink, new_channel) = emitter();
    let (rem_sink, rem_channel) = emitter();
    let bridge = bridge
        .with_history(Box::new(new_sink))
        .with_reminder_history(Box::new(rem_sink));
    let landed = batch(1);
    bridge.deliver_reminders(&landed, true);
    assert_eq!(emitted(&new_channel), [] as [NewMailBatch; 0]);
    assert_eq!(emitted(&rem_channel), vec![landed]);
}
