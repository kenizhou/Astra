// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the observer bridge and its rate limiter: a signal a user action
//! produced reaches the host at once, a streamed sync collapses to one notification
//! per surface per window, and the progress bar trails while the list leads.
//!
//! Its own file because `observer` is close to the size limit and these are timing
//! tests, which are the part that grows.

use std::{
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

use astra_app::{AppObserver, Surface};

use super::{DebouncedObserver, ObserverBridge, SurfaceChanged, SurfaceEventSink, DEBOUNCE_MS};

/// A sink that records what the host would have been told, in order.
struct ChanSink(mpsc::Sender<SurfaceChanged>);

impl SurfaceEventSink for ChanSink {
    fn surface_changed(&self, change: SurfaceChanged) {
        let _ = self.0.send(change);
    }
}

/// A rate-limited observer over a recording sink.
fn make_obs() -> (DebouncedObserver, mpsc::Receiver<SurfaceChanged>) {
    let (tx, rx) = mpsc::channel();
    let obs = DebouncedObserver::new(ObserverBridge::new(Box::new(ChanSink(tx))));
    (obs, rx)
}

/// Waits for one signal, panicking with `what` if it never arrives.
fn next(rx: &mpsc::Receiver<SurfaceChanged>, what: &str) -> SurfaceChanged {
    rx.recv_timeout(Duration::from_millis(DEBOUNCE_MS * 4))
        .unwrap_or_else(|err| panic!("no signal for {what}: {err}"))
}

#[test]
fn the_payload_names_the_surface_in_snake_case() {
    let wire = serde_json::to_value(SurfaceChanged {
        surface: Surface::MailboxList,
    })
    .expect("the payload serialises");

    assert_eq!(wire, serde_json::json!({ "surface": "mailbox_list" }));
}

#[test]
fn a_burst_of_mailbox_lists_delivers_a_leading_signal_and_one_trailing_one() {
    let (obs, rx) = make_obs();
    // Fire 20 mailbox-list signals in rapid succession, as a streamed sync does.
    let sent = Instant::now();
    for _ in 0..20 {
        obs.surface_changed(Surface::MailboxList);
    }
    // The first goes straight through, so whatever produced it is on screen now.
    let first = next(&rx, "the leading mailbox-list signal");
    assert_eq!(first.surface, Surface::MailboxList);
    assert!(
        sent.elapsed() < Duration::from_millis(DEBOUNCE_MS / 2),
        "the leading signal waited {:?}",
        sent.elapsed(),
    );
    // The other nineteen collapse into one trailing notification carrying the final
    // state; not one per signal, which is the churn this rate limiter exists for.
    let mut trailing = 0;
    while rx
        .recv_timeout(Duration::from_millis(DEBOUNCE_MS * 2))
        .is_ok()
    {
        trailing += 1;
    }
    assert_eq!(
        trailing, 1,
        "nineteen more signals delivered {trailing} time(s)",
    );
}

#[test]
fn a_lone_mailbox_list_signal_is_immediate() {
    // What a folder click is: one rebuild, one signal, nothing else in flight.
    // Coalescing has nothing to coalesce here, so the window must not be spent before
    // the host is told; that delay is on every navigation, and it is the whole
    // difference between the list moving under the finger and arriving a beat later.
    let (obs, rx) = make_obs();
    let sent = Instant::now();
    obs.surface_changed(Surface::MailboxList);
    let arrived = next(&rx, "a lone mailbox-list signal");
    let waited = sent.elapsed();
    assert_eq!(arrived.surface, Surface::MailboxList);
    assert!(
        waited < Duration::from_millis(DEBOUNCE_MS / 2),
        "a lone signal waited {waited:?}, which is the debounce window rather than \
         the leading edge",
    );
}

#[test]
fn a_sync_progress_burst_is_coalesced_to_one_trailing_signal() {
    // A streamed pass commits one message at a time and signals the progress surface
    // per commit; coalescing the list but not the bar meant every bar signal flushed
    // the list queued the instant before, so neither was ever coalesced. The bar takes
    // no leading edge because it is meant to stay down for a download that finishes
    // inside one window rather than raise and drop on every short pass: late is right
    // for a bar, and wrong for a list.
    let (obs, rx) = make_obs();
    let sent = Instant::now();
    for _ in 0..20 {
        obs.surface_changed(Surface::SyncProgress);
    }
    let mut delivered = 0;
    while rx
        .recv_timeout(Duration::from_millis(DEBOUNCE_MS * 2))
        .is_ok()
    {
        delivered += 1;
    }
    assert!(
        sent.elapsed() >= Duration::from_millis(DEBOUNCE_MS),
        "the bar went up after {:?}, inside its own window, that is the flash",
        sent.elapsed(),
    );
    assert_eq!(delivered, 1, "twenty signals delivered {delivered} time(s)");
}

#[test]
fn a_commit_signalling_both_surfaces_delivers_each_on_its_own_window() {
    // What a streamed sync actually does, twenty commits of it. Each surface keeps its
    // own window, so a commit's list signal does not spend the bar's window and vice
    // versa: the one that happened to be signalled first must not take the other's.
    let (obs, rx) = make_obs();
    for _ in 0..20 {
        obs.surface_changed(Surface::MailboxList);
        obs.surface_changed(Surface::SyncProgress);
    }
    let mut delivered = Vec::new();
    while let Ok(change) = rx.recv_timeout(Duration::from_millis(DEBOUNCE_MS * 4)) {
        delivered.push(change.surface);
    }
    let lists = delivered
        .iter()
        .filter(|surface| matches!(surface, Surface::MailboxList))
        .count();
    let bars = delivered
        .iter()
        .filter(|surface| matches!(surface, Surface::SyncProgress))
        .count();
    assert_eq!(
        (lists, bars),
        (2, 1),
        "twenty paired commits delivered {lists} list and {bars} bar signal(s)",
    );
}

#[test]
fn a_gapped_burst_repaints_at_a_bounded_rate() {
    // Signals with small gaps, each one of which used to push the deadline forward:
    // bounded repaints, and never one per signal.
    let (obs, rx) = make_obs();
    let gap = Duration::from_millis(DEBOUNCE_MS / 4);
    let feeder = Arc::new(obs);
    let thread_obs = Arc::clone(&feeder);
    thread::spawn(move || {
        for _ in 0..5 {
            thread_obs.surface_changed(Surface::MailboxList);
            thread::sleep(gap);
        }
    });
    let mut delivered = 0;
    while rx
        .recv_timeout(Duration::from_millis(DEBOUNCE_MS * 2))
        .is_ok()
    {
        delivered += 1;
    }
    assert!(
        (1..=3).contains(&delivered),
        "five signals over ~two windows delivered {delivered} time(s)",
    );
}

#[test]
fn a_flush_fires_what_is_pending_before_its_window_closes() {
    // Both surfaces this milestone ships are debounced, so nothing reaches the flush
    // from `surface_changed` yet; the next surface that is not debounced will, and it
    // must fire what is pending **first** so the host's snapshot stays consistent. The
    // flush is exercised directly: the second signal inside the window is the pending
    // one a future non-debounced surface would carry out.
    let (obs, rx) = make_obs();
    obs.surface_changed(Surface::MailboxList);
    let leading = next(&rx, "the leading signal");
    assert_eq!(leading.surface, Surface::MailboxList);

    obs.surface_changed(Surface::MailboxList);
    assert!(
        rx.recv_timeout(Duration::from_millis(DEBOUNCE_MS / 4))
            .is_err(),
        "the second signal waits for its window",
    );

    obs.flush_pending();
    let flushed = next(&rx, "the flushed signal");
    assert_eq!(
        flushed.surface,
        Surface::MailboxList,
        "the flush carries the pending signal out early",
    );
    // The window was drained, so nothing fires when it later closes.
    assert!(
        rx.recv_timeout(Duration::from_millis(DEBOUNCE_MS * 2))
            .is_err(),
        "a flushed window fires nothing at its close",
    );
}
