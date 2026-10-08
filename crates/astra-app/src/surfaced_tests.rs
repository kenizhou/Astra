// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the surfaced cell: a publish is visible the instant it signals, the pull
//! never signals, and the re-signal is the only path that announces without writing.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use super::Surfaced;
use crate::{AppObserver, Surface};

/// An observer that pulls the cell the moment it is signalled, which is exactly what a host
/// does, and the only way to catch a signal that ran ahead of its write.
struct PullsOnSignal {
    cell: Mutex<Option<Arc<Surfaced<String>>>>,
    seen: Mutex<Vec<String>>,
    surfaces: Mutex<Vec<Surface>>,
}

impl PullsOnSignal {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            cell: Mutex::new(None),
            seen: Mutex::new(Vec::new()),
            surfaces: Mutex::new(Vec::new()),
        })
    }
}

impl AppObserver for PullsOnSignal {
    fn surface_changed(&self, surface: Surface) {
        self.surfaces.lock().unwrap().push(surface);
        let cell = self.cell.lock().unwrap().clone();
        if let Some(cell) = cell {
            self.seen.lock().unwrap().push(cell.get());
        }
    }
}

fn wired() -> (Arc<Surfaced<String>>, Arc<PullsOnSignal>) {
    let observer = PullsOnSignal::new();
    let cell = Arc::new(Surfaced {
        value: Mutex::new(String::new()),
        surface: Surface::MailboxList,
        observer: observer.clone(),
    });
    *observer.cell.lock().unwrap() = Some(Arc::clone(&cell));
    (cell, observer)
}

#[test]
fn a_published_value_is_readable_at_the_moment_it_is_signalled() {
    // The whole point. A host that pulls inside `surface_changed` must see the new value; if
    // the signal were sent first, this would read the empty string.
    let (cell, observer) = wired();
    cell.publish("first".to_owned());
    cell.publish("second".to_owned());
    assert_eq!(
        *observer.seen.lock().unwrap(),
        vec!["first".to_owned(), "second".to_owned()]
    );
}

#[test]
fn every_signal_carries_the_cells_own_surface() {
    let (cell, observer) = wired();
    cell.publish("x".to_owned());
    cell.resignal();
    assert_eq!(
        *observer.surfaces.lock().unwrap(),
        vec![Surface::MailboxList; 2],
        "a cell cannot be made to announce a surface it does not own"
    );
}

#[test]
fn resignal_repeats_the_published_value_and_writes_nothing() {
    let (cell, observer) = wired();
    cell.publish("only".to_owned());
    cell.resignal();
    assert_eq!(cell.get(), "only");
    assert_eq!(
        *observer.seen.lock().unwrap(),
        vec!["only".to_owned(), "only".to_owned()],
        "a re-signal repeats what is published rather than announcing something new"
    );
}

#[test]
fn a_pull_never_signals() {
    // `get` is the host's read path and runs on every pull; if it signalled, a host that
    // reads on signal would loop forever.
    let (cell, observer) = wired();
    cell.publish("x".to_owned());
    let before = observer.surfaces.lock().unwrap().len();
    for _ in 0..5 {
        let _ = cell.get();
    }
    assert_eq!(observer.surfaces.lock().unwrap().len(), before);
}

#[test]
fn concurrent_publishers_each_signal_exactly_once() {
    let (cell, observer) = wired();
    let count = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|scope| {
        for index in 0..8 {
            let (cell, count) = (Arc::clone(&cell), Arc::clone(&count));
            scope.spawn(move || {
                cell.publish(format!("v{index}"));
                count.fetch_add(1, Ordering::SeqCst);
            });
        }
    });
    assert_eq!(count.load(Ordering::SeqCst), 8);
    assert_eq!(observer.surfaces.lock().unwrap().len(), 8);
    // Whichever won, the last value seen by the observer is a real published one, never a
    // half-written or empty cell.
    assert!(observer
        .seen
        .lock()
        .unwrap()
        .iter()
        .all(|seen| seen.starts_with('v')));
}
