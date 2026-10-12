// SPDX-License-Identifier: GPL-3.0-only
//! The people half of the runtime: the [`PeopleIntent`] dispatch over the
//! engine's people reads and contact writes, the view state a rebuild
//! projects, and the fold and monogram derivations the engine does not carry.
//!
//! The engine owns the join, the ordering, and the matching; this module owns
//! what the intents mean as product behaviour: the query the list runs under
//! (handed to the engine verbatim, never re-filtered locally, because only
//! the engine sees every person), the open person, the write states (a write
//! that reconciles to anything but `Applied` **landed**; its recovery is the
//! re-read the next refresh performs), and the two presentation facts derived
//! once here because the engine at the pinned rev carries no section or
//! avatar model: the section fold and the monogram.
//!
//! The contacts **legs** are host-installed ([`App::install_contacts`]): the
//! engine's contact verbs take a provider that implements the contacts
//! domain, while the app and its hosts are generic over the mail-and-calendar
//! provider trait alone, so a CardDAV or JMAP contacts adapter arrives as its
//! own object-safe handle beside the account's mail providers rather than
//! through them.
//!
//! [`App::install_contacts`]: crate::App::install_contacts
//! [`PeopleIntent`]: crate::protocol::PeopleIntent

pub(crate) mod avatar;
mod cards;
mod seed;
mod snapshot;
mod suggest;
mod write;

use std::collections::BTreeMap;

use astra_viewmodel::{Monogram, PeopleWriteState};
use engine_api::{AccountId, ContactsProvider, PersonId, Provider};

use crate::{App, PeopleIntent};

/// How many people one list read asks the engine for. The intent family
/// carries no paging verb, so anything past the first page is unreachable
/// until the query narrows; this is the engine's own maximum, spent whole.
pub(crate) const PEOPLE_PAGE_LIMIT: usize = 200;

/// One installed contacts leg: a provider handle that implements the engine's
/// contacts domain, boxed so the mail-generic app can drive it.
pub(crate) type ContactsLeg = Box<dyn ContactsProvider>;

/// The people view state: what a rebuild projects and the intents move. Held
/// behind one mutex because the rebuild reads all of it and every intent
/// writes exactly one field of it.
#[derive(Default)]
pub(crate) struct PeopleState {
    /// The contacts legs the host installed, per account: an install replaces
    /// the account's set (a reconnect hands over fresh handles). Empty until
    /// the host dials a contacts leg, and then the reads still work over the
    /// already-synced store while the writes and the create targets honestly
    /// have nowhere to go.
    legs: BTreeMap<AccountId, Vec<std::sync::Arc<ContactsLeg>>>,
    /// The query the list runs under, handed to the engine verbatim: empty is
    /// the unfiltered list.
    query: String,
    /// The open person, or nobody. Kept beside the snapshot so a refresh
    /// re-resolves it (including through a merge's retired id).
    open: Option<PersonId>,
    /// Each account's write state, for the snapshot's write rows. Absent
    /// means no write is in flight and none is unconfirmed or refused.
    writes: BTreeMap<AccountId, PeopleWriteState>,
}

impl PeopleState {
    /// The state every app starts with: no legs, no query, nobody open, no
    /// writes.
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

/// What one rebuild reads: cloned out of the state so no lock is held across
/// an engine read.
pub(crate) struct PeopleInputs {
    /// The installed legs, per account, in account order.
    pub legs: Vec<(AccountId, Vec<std::sync::Arc<ContactsLeg>>)>,
    /// The query the list read runs under.
    pub query: String,
    /// The open person.
    pub open: Option<PersonId>,
    /// The write rows, one per account with a state.
    pub writes: Vec<(AccountId, PeopleWriteState)>,
}

impl<P: Provider> App<P> {
    /// The people snapshot, for the host's pull after a
    /// [`Surface::People`](crate::Surface::People) signal.
    #[must_use]
    pub fn people(&self) -> astra_viewmodel::PeopleSnapshot {
        self.people.get()
    }

    /// Installs `leg` as `account`'s contacts leg, replacing whatever stood
    /// there: the reconnect path, handing the engine a fresh adapter handle.
    /// One leg per account is the rule today (a CardDAV dial); the state
    /// holds a vec because an account with a personal book beside a
    /// directory leg will need more than one, and that extension grows this
    /// API rather than the type. Call it before the first people dispatch;
    /// an app with no leg installed still lists people already in the
    /// store, and its writes have nowhere to go.
    pub fn install_contacts(&self, account: &AccountId, leg: ContactsLeg) {
        self.people_state
            .lock()
            .expect("people mutex poisoned")
            .legs
            .insert(account.clone(), vec![std::sync::Arc::new(leg)]);
    }

    /// Tears the account's contacts legs out: the removal a cleared dav
    /// section and a removed account perform, so the reads stop offering an
    /// account whose writes have nowhere to go. A no-op for an account with
    /// no leg standing.
    pub fn remove_contacts(&self, account: &AccountId) {
        self.people_state
            .lock()
            .expect("people mutex poisoned")
            .legs
            .remove(account);
    }

    /// The decision inputs for one rebuild.
    pub(crate) fn people_inputs(&self) -> PeopleInputs {
        let state = self.people_state.lock().expect("people mutex poisoned");
        PeopleInputs {
            legs: state
                .legs
                .iter()
                .map(|(account, legs)| (account.clone(), legs.clone()))
                .collect(),
            query: state.query.clone(),
            open: state.open,
            writes: state.writes.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        }
    }

    /// `account`'s installed legs, cloned out under the lock.
    pub(crate) fn people_legs(&self, account: &AccountId) -> Vec<std::sync::Arc<ContactsLeg>> {
        self.people_state
            .lock()
            .expect("people mutex poisoned")
            .legs
            .get(account)
            .cloned()
            .unwrap_or_default()
    }

    /// Routes one [`PeopleIntent`] to its handler. Every arm publishes the
    /// people surface on completion, because every arm moves something the
    /// snapshot shows (the query, the open person, a write's outcome) or the
    /// create targets riding beside them.
    pub(crate) async fn dispatch_people(&self, intent: PeopleIntent) {
        match intent {
            PeopleIntent::Refresh => self.refresh_people().await,
            PeopleIntent::Search { query } => {
                self.people_state
                    .lock()
                    .expect("people mutex poisoned")
                    .query = query;
                self.rebuild_people().await;
            }
            PeopleIntent::OpenPerson { person } => {
                self.people_state
                    .lock()
                    .expect("people mutex poisoned")
                    .open = Some(person);
                self.rebuild_people().await;
            }
            PeopleIntent::ClosePerson => {
                self.people_state
                    .lock()
                    .expect("people mutex poisoned")
                    .open = None;
                self.rebuild_people().await;
            }
            PeopleIntent::CreateContact { book, draft } => {
                self.create_person_contact(&book, &draft).await;
            }
            PeopleIntent::UpdateContact { card, patch } => {
                self.update_person_contact(&card, &patch).await;
            }
        }
    }

    /// Syncs every installed contacts leg and rebuilds. This is also the
    /// recovery a `FailedWithRetry` row waits for: a successful re-read is
    /// the one thing that resolves an unconfirmed-but-landed write, and a
    /// refresh that lands one clears the row for the account it re-read.
    async fn refresh_people(&self) {
        for (account, legs) in self.people_inputs().legs {
            let at = self.account_ordinal(&account).await;
            let mut synced = false;
            for leg in &legs {
                match self.engine.sync_contacts(leg.as_ref(), &account).await {
                    Ok(_) => synced = true,
                    Err(engine_api::ApiError::Busy) => {
                        log::info!("people[a{at}]: another sync holds the contacts; skipped");
                    }
                    Err(err) => {
                        log::warn!("people[a{at}]: the contacts sync failed: {err}");
                    }
                }
            }
            if synced {
                // A landed sync resolves an unconfirmed write (its recovery
                // is the re-read), but the Invalid sentence belongs to the
                // form the user is still looking at: only a later write
                // clears it, never a background refresh that landed while
                // the form sat unsubmitted.
                let resolves = {
                    let writes = &self
                        .people_state
                        .lock()
                        .expect("people mutex poisoned")
                        .writes;
                    writes
                        .get(&account)
                        .is_none_or(|state| !matches!(state, PeopleWriteState::Invalid))
                };
                if resolves {
                    self.clear_people_write(&account);
                }
            }
        }
        self.rebuild_people().await;
    }
}

/// The monogram fill palette, pinned: the rebuild only ever emits a slot from
/// this finite set, so every surface that shows one person paints the same
/// colour and a theme change is one edit here.
pub(crate) const MONOGRAM_PALETTE: [&str; 8] = [
    "#E57373", "#F06292", "#BA68C8", "#7986CB", "#64B5F6", "#4DB6AC", "#AED581", "#FFD54F",
];

/// The fold of one person's identity string (their name, or their address
/// when the sources carry no name): the section bucket the row files under
/// and the order key the list lays out by.
///
/// The engine orders a page by the raw code points of the lowercased display
/// string and does not fold, so Émile lands after Emma; the rebuild re-orders
/// the page's rows by this key instead, and the sections and the order agree.
/// The fold covers the Latin alphabet (an accented letter is its base letter,
/// so Émile files between Emil and Emma and Øystein under O); every other
/// script, digit, and symbol collects in the one `#` bucket, which lays out
/// after the alphabet. The monogram is **not** folded: it keeps the real
/// characters the name carries.
pub(crate) struct FoldKey {
    /// `0` for the alphabet, `1` for the `#` bucket, which sorts after it.
    bucket: u8,
    /// The folded, lowercased identity for the alphabet bucket; the raw
    /// lowercased identity for `#`.
    key: String,
}

/// Folds `identity` (already the engine's own display pick: the name, else
/// the address) into its section bucket and order key.
pub(crate) fn fold_identity(identity: &str) -> FoldKey {
    let lower = identity.to_lowercase();
    let mut key = String::with_capacity(lower.len());
    let mut bucket = 1_u8;
    for (at, character) in lower.chars().enumerate() {
        if let Some(base) = fold_letter(character) {
            key.push(base.to_ascii_lowercase());
            if at == 0 {
                bucket = 0;
            }
        } else {
            key.push(character);
        }
    }
    FoldKey { bucket, key }
}

/// The section heading one folded identity files under: an uppercased letter,
/// or `#` for anything that does not start with one.
pub(crate) fn section_of(fold: &FoldKey) -> String {
    match fold.bucket {
        0 => match fold.key.chars().next() {
            Some(letter) => letter.to_ascii_uppercase().to_string(),
            None => "#".to_owned(),
        },
        _ => "#".to_owned(),
    }
}

/// One character's base letter, when it is a Latin letter: ASCII directly,
/// and the accented and extended Latin forms (Latin-1 Supplement and Latin
/// Extended-A) folded onto the base they stand on. Everything else, and every
/// other script, folds to nothing.
fn fold_letter(character: char) -> Option<char> {
    match character {
        'A'..='Z' | 'a'..='z' => Some(character.to_ascii_uppercase()),
        // The accented forms plus Æ and Ø, which file under their leading
        // base letter per the fold rule, and Latin Extended-A (macrons,
        // breves, ogoneks, the ligatures) folded the same way.
        'À'..='Å' | 'à'..='å' | 'Æ' | 'æ' | '\u{0100}'..='\u{0105}' => Some('A'),
        'Ç' | 'ç' | '\u{0106}'..='\u{010D}' => Some('C'),
        'Ð' | 'ð' | '\u{010E}'..='\u{0111}' => Some('D'),
        'È'..='Ë' | 'è'..='ë' | '\u{0112}'..='\u{011B}' => Some('E'),
        'Ì'..='Ï' | 'ì'..='ï' | '\u{0128}'..='\u{0133}' => Some('I'),
        '\u{011C}'..='\u{0123}' => Some('G'),
        '\u{0124}'..='\u{0127}' => Some('H'),
        '\u{0134}'..='\u{0135}' => Some('J'),
        '\u{0136}'..='\u{0138}' => Some('K'),
        '\u{0139}'..='\u{0142}' => Some('L'),
        'Ñ' | 'ñ' | '\u{0143}'..='\u{014B}' => Some('N'),
        'Ò'..='Ö' | 'ò'..='ö' | 'Ø' | 'ø' | '\u{014C}'..='\u{0153}' => Some('O'),
        '\u{0154}'..='\u{0159}' => Some('R'),
        'ß' | '\u{015A}'..='\u{0161}' | '\u{017F}' => Some('S'),
        'Þ' | 'þ' | '\u{0162}'..='\u{0167}' => Some('T'),
        'Ù'..='Ü' | 'ù'..='ü' | '\u{0168}'..='\u{0173}' => Some('U'),
        '\u{0174}'..='\u{0175}' => Some('W'),
        'Ý' | 'ý' | 'ÿ' | '\u{0176}'..='\u{0178}' => Some('Y'),
        '\u{0179}'..='\u{017E}' => Some('Z'),
        _ => None,
    }
}

/// The monogram one person's avatar draws: the initials and the palette fill.
///
/// The initials are the first character of the first and last words of the
/// name, uppercased and otherwise untouched (Émile keeps his É); the
/// address's first character when the sources carry no name; empty when they
/// carry neither, the signal for the host to draw its own person glyph. The
/// fill is keyed on the lowercased primary address so it is stable per
/// person, with the person id as the fallback key when there is no address:
/// a person cannot change colour between surfaces, and two people who share
/// a colour still differ in initials.
pub(crate) fn monogram_of(name: &str, address: Option<&str>, person: PersonId) -> Monogram {
    Monogram {
        initials: initials_of(name, address),
        colour: colour_of(address, person).to_owned(),
    }
}

/// The initials the monogram draws, per the rule above.
fn initials_of(name: &str, address: Option<&str>) -> String {
    let mut words = name.split_whitespace();
    let head = words.next();
    let tail = words.next_back();
    match (head, tail) {
        (Some(first), Some(last)) => {
            let mut initials = head_uppercased(first);
            initials.push_str(&head_uppercased(last));
            initials
        }
        (Some(only), None) => head_uppercased(only),
        (None, _) => address.map(head_uppercased).unwrap_or_default(),
    }
}

/// The uppercased form of one word's leading character.
fn head_uppercased(word: &str) -> String {
    word.chars()
        .next()
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_default()
}

/// The palette slot one person's colour key picks.
fn colour_of(address: Option<&str>, person: PersonId) -> &'static str {
    let key = match address {
        Some(address) => address.to_lowercase(),
        None => person.get().to_string(),
    };
    let slot = fnv1a(key.as_bytes()) % MONOGRAM_PALETTE.len() as u64;
    MONOGRAM_PALETTE[usize::try_from(slot).expect("the slot is under the palette length")]
}

/// FNV-1a over the bytes, the same constants the engine's query signatures
/// hash with: fixed and stable across runs and builds, which the standard
/// library's hasher deliberately is not, and a person's colour must not move.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        hash.wrapping_mul(0x0100_0000_01b3) ^ u64::from(*byte)
    })
}

#[cfg(test)]
#[path = "../people_dispatch_tests.rs"]
mod dispatch_tests;
