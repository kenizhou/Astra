// SPDX-License-Identifier: GPL-3.0-only
//! The settings slice's fixtures: the scripted account store the settings
//! tests drive over the account-info seam, plus the app builder that
//! installs it. The fake models the host's dial table and keyring as one
//! thing: the infos the read half answers, the merged dav sections the write
//! half stores, and the scripted failures the row's error states report.

#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use engine_api::{AccountId, ContactsProvider, Engine};

use crate::calendar_fakes::CalendarFake;
use crate::people_fakes::PeopleFake;
use crate::protocol::DavSectionInput;
use crate::settings::{AccountInfo, DavLegs, HostAccountOps};
use crate::{Account, App, AppObserver, Surface};

/// The scripted account store the settings tests drive: infos served from a
/// map the writes refresh, dav sections merged and stored the way a host's
/// keyring holds them, dials that succeed or fail per script, and removals
/// recorded in order.
pub(crate) struct FakeAccountOps {
    /// The infos the read half answers, keyed by account id. The write
    /// halves refresh the entry, so the rows a rebuild renders track what
    /// the fake stores.
    infos: Mutex<BTreeMap<String, AccountInfo>>,
    /// The stored sections, keyed by account id: the merged editor input
    /// over what stood, password included (the credential the app never
    /// sees). `None` for an account with no section.
    stored: Mutex<BTreeMap<String, StoredSection>>,
    /// The accounts whose next `store_dav` fails, with the sentence the row
    /// shows. One shot: a later save may succeed.
    fail_dials: Mutex<BTreeMap<String, String>>,
    /// Whether the next `remove_account` fails.
    fail_removal: Mutex<bool>,
    /// Every recorded removal, in order.
    removals: Mutex<Vec<String>>,
    /// The kind a fresh account's info carries.
    kind: astra_viewmodel::AccountKind,
    /// The address a fresh account's info carries.
    address: String,
    /// The standing grant refusal states the infos answer, keyed by
    /// account id.
    grants: BTreeMap<String, astra_viewmodel::GrantState>,
}

/// The sentence a merged section that names no origin answers, the host's
/// own wording so the row reads the same whichever half refused.
const NO_ORIGIN: &str = "the merged section names no origin to dial";
/// The stored section as the fake holds it: the merged editor input, with
/// the password the echo answers `password_set` from.
#[derive(Clone, Default)]
struct StoredSection {
    /// The CalDAV origin, when the section names one.
    caldav_url: Option<String>,
    /// The CardDAV origin, when the section names one of its own.
    carddav_url: Option<String>,
    /// The login both legs present.
    username: String,
    /// The stored password, when one has ever been set.
    password: Option<String>,
}

/// The blank-trimmed half of an origin pair, the discipline the config
/// layer's own accessor follows.
fn non_blank(value: Option<&String>) -> Option<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned())
}

impl StoredSection {
    /// The merged section an editor input leaves standing: each field from
    /// the input when the form states one, from the store otherwise.
    fn merged(base: Option<&Self>, input: &DavSectionInput) -> Self {
        let base = base.cloned().unwrap_or_default();
        Self {
            caldav_url: match non_blank(input.caldav_url.as_ref()) {
                Some(url) => Some(url),
                None if input.caldav_url.is_some() => None,
                None => base.caldav_url,
            },
            carddav_url: match non_blank(input.carddav_url.as_ref()) {
                Some(url) => Some(url),
                None if input.carddav_url.is_some() => None,
                None => base.carddav_url,
            },
            username: input
                .username
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(base.username),
            password: input.password.clone().or(base.password),
        }
    }

    /// The CardDAV origin after the fallback: its own, else the CalDAV one.
    fn carddav_origin(&self) -> Option<String> {
        non_blank(self.carddav_url.as_ref()).or_else(|| non_blank(self.caldav_url.as_ref()))
    }

    /// The echo the settings row renders: the stored section minus the
    /// credential, stating only that one is stored.
    fn echo(&self) -> astra_viewmodel::DavSectionRow {
        astra_viewmodel::DavSectionRow {
            caldav_url: non_blank(self.caldav_url.as_ref()),
            carddav_url: non_blank(self.carddav_url.as_ref()),
            username: self.username.clone(),
            dav_password_set: self.password.is_some(),
        }
    }
}

impl FakeAccountOps {
    /// An empty store that describes no accounts.
    pub(crate) fn new() -> Self {
        Self {
            infos: Mutex::new(BTreeMap::new()),
            stored: Mutex::new(BTreeMap::new()),
            fail_dials: Mutex::new(BTreeMap::new()),
            fail_removal: Mutex::new(false),
            removals: Mutex::new(Vec::new()),
            kind: astra_viewmodel::AccountKind::ImapSmtp,
            address: "info@test.local".to_owned(),
            grants: BTreeMap::new(),
        }
    }

    /// Describes one account to the read half, with no dav section.
    pub(crate) fn describing(self, id: &str) -> Self {
        self.infos.lock().unwrap().insert(
            id.to_owned(),
            AccountInfo {
                id: AccountId::try_from(id).unwrap(),
                kind: self.kind,
                address: self.address.clone(),
                grant: None,
                dav: None,
                crypto_method: "none".to_owned(),
            },
        );
        self
    }

    /// Describes one account under one address and kind, the shape the
    /// refusal-row tests name a dead grant with.
    pub(crate) fn describing_as(
        mut self,
        id: &str,
        address: &str,
        kind: astra_viewmodel::AccountKind,
    ) -> Self {
        self.address = address.to_owned();
        self.kind = kind;
        self.describing(id)
    }

    /// States the standing grant refusal the account's info answers.
    pub(crate) fn with_grant(mut self, id: &str, grant: astra_viewmodel::GrantState) -> Self {
        if let Some(info) = self.infos.lock().unwrap().get_mut(id) {
            info.grant = Some(grant);
        }
        self.grants.insert(id.to_owned(), grant);
        self
    }

    /// Sets the kind every described account carries.
    pub(crate) fn with_kind(mut self, kind: astra_viewmodel::AccountKind) -> Self {
        self.kind = kind;
        self
    }

    /// Seeds one account's stored section, the shape a previous session
    /// left behind.
    pub(crate) fn storing(
        &self,
        id: &str,
        caldav_url: Option<&str>,
        carddav_url: Option<&str>,
        username: &str,
        password: Option<&str>,
    ) {
        let section = StoredSection {
            caldav_url: caldav_url.map(str::to_owned),
            carddav_url: carddav_url.map(str::to_owned),
            username: username.to_owned(),
            password: password.map(str::to_owned),
        };
        self.refresh_info(id, &section);
        self.stored.lock().unwrap().insert(id.to_owned(), section);
    }

    /// Scripts the account's next dav write to fail with `sentence`.
    pub(crate) fn failing_dial(&self, id: &str, sentence: &str) {
        self.fail_dials
            .lock()
            .unwrap()
            .insert(id.to_owned(), sentence.to_owned());
    }

    /// Scripts the next removal to fail.
    pub(crate) fn failing_removal(&self) {
        *self.fail_removal.lock().unwrap() = true;
    }

    /// Every recorded removal, in order.
    pub(crate) fn removals(&self) -> Vec<String> {
        self.removals.lock().unwrap().clone()
    }

    /// Whether the fake still stores a section for the account, and the
    /// username it carries.
    pub(crate) fn stored_section(&self, id: &str) -> Option<String> {
        self.stored
            .lock()
            .unwrap()
            .get(id)
            .map(|section| section.username.clone())
    }

    /// Refreshes the info the read half answers for one account from the
    /// section the fake stores.
    fn refresh_info(&self, id: &str, section: &StoredSection) {
        let echo = section.echo();
        let mut infos = self.infos.lock().unwrap();
        infos.entry(id.to_owned()).and_modify(|info| {
            info.dav = Some(echo.clone());
        });
    }
}

/// The provider a successful re-dial appends: a calendar-capable fake, the
/// engine-side half of a dialled CalDAV leg.
fn dialled_calendar() -> CalendarFake {
    CalendarFake::with_calendar_and_mail()
}

/// The contacts leg a successful re-dial installs: the engine-side half of a
/// dialled CardDAV leg.
fn dialled_contacts() -> PeopleFake {
    PeopleFake::with_book()
}

#[async_trait]
impl HostAccountOps<CalendarFake> for FakeAccountOps {
    async fn account_infos(&self) -> Vec<AccountInfo> {
        // The refusal states overlay at read time, so a `with_grant`
        // scripted before or after the `describing` it names reads the
        // same.
        let grants = self.grants.clone();
        let mut infos: Vec<AccountInfo> = self.infos.lock().unwrap().values().cloned().collect();
        for info in &mut infos {
            info.grant = grants.get(info.id.as_str()).copied();
        }
        infos
    }

    async fn store_dav(
        &self,
        account: &AccountId,
        section: Option<DavSectionInput>,
    ) -> Result<DavLegs<CalendarFake>, String> {
        if let Some(sentence) = self.fail_dials.lock().unwrap().remove(account.as_str()) {
            return Err(sentence);
        }
        let id = account.as_str().to_owned();
        let Some(input) = section else {
            self.stored.lock().unwrap().remove(&id);
            if let Some(info) = self.infos.lock().unwrap().get_mut(&id) {
                info.dav = None;
            }
            return Ok(DavLegs {
                caldav: None,
                carddav: None,
            });
        };
        let merged = {
            let stored = self.stored.lock().unwrap();
            StoredSection::merged(stored.get(&id), &input)
        };
        let caldav_origin = non_blank(merged.caldav_url.as_ref());
        let carddav_origin = merged.carddav_origin();
        if caldav_origin.is_none() && carddav_origin.is_none() {
            return Err(NO_ORIGIN.to_owned());
        }
        // The host's dial refuses a merged section with no password; the
        // fake answers the same refusal so the suite never pins a path the
        // shipping host cannot produce.
        if merged.password.is_none() {
            return Err("the merged section names no credential to dial".to_owned());
        }
        self.refresh_info(&id, &merged);
        self.stored.lock().unwrap().insert(id, merged);
        Ok(DavLegs {
            caldav: caldav_origin.is_some().then(dialled_calendar),
            carddav: carddav_origin
                .is_some()
                .then(|| Box::new(dialled_contacts()) as Box<dyn ContactsProvider>),
        })
    }

    async fn remove_account(&self, account: &AccountId) -> Result<(), String> {
        self.removals
            .lock()
            .unwrap()
            .push(account.as_str().to_owned());
        if *self.fail_removal.lock().unwrap() {
            *self.fail_removal.lock().unwrap() = false;
            return Err("the stored credential could not be removed".to_owned());
        }
        let id = account.as_str();
        self.stored.lock().unwrap().remove(id);
        self.infos.lock().unwrap().remove(id);
        Ok(())
    }

    async fn store_crypto_method(&self, account: &AccountId, method: &str) -> Result<(), String> {
        let id = account.as_str();
        let mut infos = self.infos.lock().unwrap();
        let Some(info) = infos.get_mut(id) else {
            return Err("the account has no stored config on this host".to_owned());
        };
        info.crypto_method = method.to_owned();
        Ok(())
    }
}

/// Wraps `providers` as an account `id` (its identity is
/// `info@test.local`, the address the calendar fixtures share).
pub(crate) fn account(id: &str, providers: Vec<CalendarFake>) -> Account<CalendarFake> {
    Account {
        id: AccountId::try_from(id).unwrap(),
        providers,
        identity: engine_api::EmailAddress::new("info@test.local"),
    }
}

/// Builds an in-memory app over `accounts` with the scripted `ops` installed
/// and the preferences persisted to `prefs`, recording signalled surfaces.
/// The scripted store arrives shared, so the test keeps the handle it
/// scripts through beside the app that reads it.
pub(crate) fn settings_app(
    accounts: Vec<Account<CalendarFake>>,
    ops: Arc<FakeAccountOps>,
    prefs: Option<PathBuf>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> App<CalendarFake> {
    let app = App::new(
        Arc::new(Engine::open_in_memory().unwrap()),
        accounts,
        None,
        Arc::new(RecordingObserver {
            surfaces: Arc::clone(surfaces),
        }),
    )
    .with_prefs_path(prefs);
    app.install_account_ops(ops);
    app
}

/// Records the surfaces the app signals, so a test can assert the loop
/// fired.
pub(crate) struct RecordingObserver {
    /// Every surface signalled so far, in order.
    pub surfaces: Arc<Mutex<Vec<Surface>>>,
}

impl AppObserver for RecordingObserver {
    fn surface_changed(&self, surface: Surface) {
        self.surfaces.lock().unwrap().push(surface);
    }
}

/// The mail half of a split account, the shape every IMAP provider has: a
/// calendar fake stripped of its calendar leg.
pub(crate) fn mail_half() -> CalendarFake {
    CalendarFake::with_calendar_and_mail().mail_only()
}
