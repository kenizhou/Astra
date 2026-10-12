// SPDX-License-Identifier: GPL-3.0-only
//! The people slice's fixtures: a contacts-leg provider the people tests
//! drive through the real engine over an in-memory store, plus the card
//! builders every people test starts from.
//!
//! The server-side copy the fake holds is **mutated by the writes it
//! accepts** the way a real CardDAV server's is, so the post-write reconcile
//! a facade write runs presents the change and the store the snapshot reads
//! holds what the user did. A scripted count of refetch refusals makes a
//! write's reconcile fail locally, which is the landed-but-unconfirmed shape
//! a `FailedWithRetry` row reports.

#![allow(dead_code)]

mod fixtures;
mod provider;

pub(crate) use fixtures::{
    bare_card, book, named_card, structured_card, with_organisation, with_photo,
};

use std::sync::{Arc, Mutex};

use engine_api::{AccountId, ContactCard, ContactDraft, ContactPatch};

use crate::{Account, App, AppObserver, Surface};

/// The contacts-leg provider the people tests drive: one address book, a
/// served card set mutated by accepted writes, a scriptable write refusal,
/// and a scriptable refetch refusal.
#[derive(Clone)]
pub(crate) struct PeopleFake {
    /// The account's address books, served on every discovery pass.
    pub address_books: Vec<engine_api::AddressBook>,
    /// The server's cards, served on every card pass and mutated by the
    /// writes the fake accepts. Behind a mutex because the writes move it.
    pub cards: Arc<Mutex<Vec<ContactCard>>>,
    /// Whether the destination accepts writes: `false` is the read-only leg.
    pub writable: bool,
    /// How many writes to refuse ("the write was rejected"), so an engine
    /// refusal can be scripted.
    pub fail_writes: Arc<Mutex<u32>>,
    /// How many post-write refetches to refuse, so a write that landed can
    /// be left unconfirmed locally.
    pub fail_fetches: Arc<Mutex<u32>>,
    /// Every recorded create, in order.
    pub creates: Arc<Mutex<Vec<ContactDraft>>>,
    /// Every recorded `(base, patch)`, in order.
    pub patches: Arc<Mutex<Vec<(ContactCard, ContactPatch)>>>,
}

impl PeopleFake {
    /// A provider holding one writable default book and no cards yet: the
    /// account shape every people test starts from.
    pub(crate) fn with_book() -> Self {
        Self {
            address_books: vec![book("book-1", "Work contacts")],
            cards: Arc::new(Mutex::new(Vec::new())),
            writable: true,
            fail_writes: Arc::new(Mutex::new(0)),
            fail_fetches: Arc::new(Mutex::new(0)),
            creates: Arc::new(Mutex::new(Vec::new())),
            patches: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Deposits `card` on the server's copy: the shape a server of its own
    /// put there.
    pub(crate) fn holding(self, card: ContactCard) -> Self {
        self.cards.lock().unwrap().push(card);
        self
    }

    /// Replaces the served card of the same id: a change another client made
    /// after the last sync.
    pub(crate) fn serving(&self, card: ContactCard) {
        let mut cards = self.cards.lock().unwrap();
        if let Some(at) = cards.iter().position(|served| served.id == card.id) {
            cards[at] = card;
        }
    }

    /// Marks the leg read-only: no destination, no writes.
    pub(crate) fn readonly(self) -> Self {
        Self {
            writable: false,
            ..self
        }
    }

    /// Scripts the next `count` writes to fail.
    pub(crate) fn failing_writes(self, count: u32) -> Self {
        *self.fail_writes.lock().unwrap() = count;
        self
    }

    /// Scripts the next `count` post-write refetches to fail.
    pub(crate) fn failing_fetches(self, count: u32) -> Self {
        *self.fail_fetches.lock().unwrap() = count;
        self
    }

    /// Consumes one scripted write refusal, reporting whether the write may
    /// proceed. A refused write is never recorded: a server that rejects a
    /// request has done nothing with it.
    fn consume_write_refusal(&self) -> bool {
        let mut left = self.fail_writes.lock().unwrap();
        if *left > 0 {
            *left -= 1;
            return false;
        }
        true
    }
}

/// Wraps `provider` as an account `id`, the identity every fixture shares.
pub(crate) fn account(id: &str, provider: PeopleFake) -> Account<PeopleFake> {
    Account {
        id: AccountId::try_from(id).unwrap(),
        providers: vec![provider],
        identity: engine_api::EmailAddress::new("info@test.local"),
    }
}

/// Builds an in-memory app over `accounts`, recording signalled surfaces.
/// Each account's fake is installed as its contacts leg by the test, beside
/// this call.
pub(crate) fn people_app(
    accounts: Vec<Account<PeopleFake>>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> App<PeopleFake> {
    people_app_over(
        Arc::new(engine_api::Engine::open_in_memory().unwrap()),
        accounts,
        surfaces,
    )
}

/// The same app beside the engine it runs over, so a test can drive an
/// engine read the app does not wrap (the photo cache's filler).
pub(crate) fn people_app_with_engine(
    accounts: Vec<Account<PeopleFake>>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> (App<PeopleFake>, Arc<engine_api::Engine>) {
    let engine = Arc::new(engine_api::Engine::open_in_memory().unwrap());
    let app = people_app_over(Arc::clone(&engine), accounts, surfaces);
    (app, engine)
}

/// The shared assembly behind both builders.
fn people_app_over(
    engine: Arc<engine_api::Engine>,
    accounts: Vec<Account<PeopleFake>>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> App<PeopleFake> {
    App::new(
        engine,
        accounts,
        None,
        Arc::new(RecordingObserver {
            surfaces: Arc::clone(surfaces),
        }),
    )
}

/// Records the surfaces the app signals, so a test can assert the loop fired.
pub(crate) struct RecordingObserver {
    /// Every surface signalled so far, in order.
    pub surfaces: Arc<Mutex<Vec<Surface>>>,
}

impl AppObserver for RecordingObserver {
    fn surface_changed(&self, surface: Surface) {
        self.surfaces.lock().unwrap().push(surface);
    }
}
