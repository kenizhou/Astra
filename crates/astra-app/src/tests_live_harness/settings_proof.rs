// SPDX-License-Identifier: GPL-3.0-only
//! The shared fixtures of the gated settings proofs: the in-suite host
//! account store and the one-account mail-only suite the dav-editor
//! proofs drive.
//!
//! **The mirroring this stand-in owes.** The shipping [`HostAccountOps`]
//! lives in the desktop host (`client/desktop/src/account_ops.rs`) over
//! its dial table and the OS credential store, and the live suite lives
//! in `astra-app` and cannot use it. The store here mirrors the
//! desktop's rule EXACTLY: the same field-by-field merge
//! ([`merged_dav`]: a stated field replaces, an absent field keeps,
//! blank text states a cleared origin, a blank or absent username keeps
//! the stored login, the password keeps whenever the input states none),
//! the same no-origin refusal, and the same dial-before-persist
//! ordering (the merged config dials through the real
//! `connect_dav_providers` first, and only a dial that answered legs is
//! written to the config document), so a failed re-dial leaves the
//! stored document exactly as it was. Only the store's shape differs:
//! one TOML config document per account in a scratch directory, written
//! and read back through the same `AccountConfig::to_toml` and `load`
//! round trip the desktop's credential store performs, rather than the
//! keyring itself. Every verb reads the document from disk, so the
//! truths the proofs observe are the ones a restart would load.

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use engine_api::{AccountId, Engine, Provider};

use astra_account::{connect_dav_providers, AccountConfig, DavAccount, Secret};
use astra_viewmodel::{AccountKind, DavSectionRow};

use crate::protocol::{DavSectionInput, SettingsIntent};
use crate::settings::{AccountInfo, DavLegs, HostAccountOps};
use crate::{Account, App, AppObserver, Intent};

use super::calendar_proof::{encode_path_segment, run_token};
use super::CountingObserver;

/// The sentence a merged section that names no origin answers, the
/// desktop's own wording so a row reads the same whichever half refused.
const NO_ORIGIN: &str = "the merged section names no origin to dial";

/// The sentence a verb naming an account the store holds no document
/// for answers, the desktop's own wording.
const NOT_REGISTERED: &str = "the account has no stored config on this host";

/// The host's account store the settings proofs install: one TOML config
/// document per account in a scratch directory, standing in for the
/// desktop's dial table and credential store on the mirroring terms the
/// module header records. The dial is the real one; only the document is
/// a scratch file.
pub(super) struct DocumentAccountStore {
    /// The scratch directory holding one config document per account.
    dir: PathBuf,
}

impl DocumentAccountStore {
    /// Writes each `config`'s document into a fresh scratch directory and
    /// answers the store over it.
    pub(super) fn installing(configs: Vec<AccountConfig>) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "astra-settings-proof-{}-{}",
            std::process::id(),
            run_token()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scratch config directory");
        for config in configs {
            let id = config.account_id().expect("the account id");
            let document = config.to_toml().expect("the config document");
            std::fs::write(Self::document(&dir, id.as_str()), document)
                .expect("the config document writes");
        }
        Self { dir }
    }

    /// The document holding `id`'s account, inside `dir`.
    fn document(dir: &std::path::Path, id: &str) -> PathBuf {
        dir.join(format!("{}.toml", encode_path_segment(id)))
    }

    /// This store's document for `id`.
    fn of(&self, id: &str) -> PathBuf {
        Self::document(&self.dir, id)
    }
}

impl Drop for DocumentAccountStore {
    /// Withdraws the scratch directory with the store, so a proof run
    /// leaves no residue on the host.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The merged section an editor input leaves standing over `base`: the
/// desktop's merge rule, mirrored field for field. Each stated field
/// replaces, each absent field keeps, the password keeping whenever the
/// input states none.
fn merged_dav(base: Option<&DavAccount>, input: &DavSectionInput) -> DavAccount {
    DavAccount {
        caldav_url: match input.caldav_url.as_deref() {
            Some(url) => non_blank(Some(url)),
            None => base.and_then(|dav| dav.caldav_url.clone()),
        },
        carddav_url: match input.carddav_url.as_deref() {
            Some(url) => non_blank(Some(url)),
            None => base.and_then(|dav| dav.carddav_url.clone()),
        },
        username: match input.username.as_deref() {
            Some(name) if !name.trim().is_empty() => name.to_owned(),
            _ => base.map_or_else(String::new, |dav| dav.username.clone()),
        },
        password: match &input.password {
            Some(password) => Some(Secret::new(password.clone())),
            None => base.and_then(|dav| dav.password.clone()),
        },
    }
}

/// The masked echo the settings row renders, the desktop's own
/// projection: the stored section minus the credential, stating only
/// that one is stored.
fn dav_echo(dav: &DavAccount) -> DavSectionRow {
    DavSectionRow {
        caldav_url: non_blank(dav.caldav_url.as_deref()),
        carddav_url: non_blank(dav.carddav_url.as_deref()),
        username: dav.username.clone(),
        dav_password_set: dav.password.is_some(),
    }
}

/// `Some` for text that names something, trimmed; the config layer's own
/// blank discipline.
fn non_blank(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned())
}

#[async_trait]
impl HostAccountOps<Box<dyn Provider>> for DocumentAccountStore {
    async fn account_infos(&self) -> Vec<AccountInfo> {
        let mut infos = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return infos;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            let Ok(config) = astra_account::load(&path) else {
                log::warn!("settings: a stored account document could not be loaded");
                continue;
            };
            let account_id = match config.account_id() {
                Ok(account_id) => account_id,
                Err(err) => {
                    log::warn!("settings: a stored account could not derive its id: {err}");
                    continue;
                }
            };
            infos.push(AccountInfo {
                id: account_id,
                kind: AccountKind::ImapSmtp,
                address: config
                    .imap
                    .as_ref()
                    .map(|imap| imap.username.clone())
                    .unwrap_or_default(),
                grant: None,
                dav: config.dav.as_ref().map(dav_echo),
                crypto_method: config
                    .crypto
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), |crypto| crypto.method.clone()),
            });
        }
        infos
    }

    async fn store_dav(
        &self,
        account: &AccountId,
        section: Option<DavSectionInput>,
    ) -> Result<DavLegs<Box<dyn Provider>>, String> {
        let id = account.as_str().to_owned();
        let mut config =
            astra_account::load(self.of(&id)).map_err(|_| NOT_REGISTERED.to_owned())?;
        match section {
            None => config.dav = None,
            Some(input) => {
                let merged = merged_dav(config.dav.as_ref(), &input);
                if merged.caldav_origin().is_none() && merged.carddav_origin().is_none() {
                    return Err(NO_ORIGIN.to_owned());
                }
                config.dav = Some(merged);
            }
        }
        // The dial runs before the persist, the ordering the contract
        // rests on: an `Err` from either half leaves the stored document
        // exactly as it was.
        let providers = connect_dav_providers(&config)
            .await
            .map_err(|err| err.to_string())?;
        let document = config.to_toml().map_err(|err| err.to_string())?;
        std::fs::write(self.of(&id), document).map_err(|err| err.to_string())?;
        Ok(DavLegs {
            caldav: providers.caldav,
            carddav: providers.carddav,
        })
    }

    async fn remove_account(&self, account: &AccountId) -> Result<(), String> {
        std::fs::remove_file(self.of(account.as_str())).map_err(|err| err.to_string())
    }

    async fn store_crypto_method(
        &self,
        account: &AccountId,
        method: &str,
    ) -> Result<Option<Vec<Box<dyn Provider>>>, String> {
        let id = account.as_str().to_owned();
        let mut config =
            astra_account::load(self.of(&id)).map_err(|_| NOT_REGISTERED.to_owned())?;
        config.crypto = match method {
            "none" => None,
            other => Some(astra_account::CryptoAccount {
                method: other.to_owned(),
            }),
        };
        let document = config.to_toml().map_err(|err| err.to_string())?;
        std::fs::write(self.of(&id), document).map_err(|err| err.to_string())?;
        // The proof store re-dials nothing: the write answers no new providers.
        Ok(None)
    }
}

/// The settings suite: one harness account whose app starts mail-only
/// (the accepted-certificate dial, no dav section anywhere) with the
/// document store installed over its config, so the proofs drive the
/// real seam. Returns the app, the account's id, and the store.
pub(super) async fn settings_suite(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (App<Box<dyn Provider>>, AccountId, Arc<DocumentAccountStore>) {
    let (account_id, providers, ..) =
        super::dial_with_accepted_certificate(host, port, user, password).await;
    let config = super::account(host, port, user, password, Vec::new());
    let store = Arc::new(DocumentAccountStore::installing(vec![config]));
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account_id.clone(),
            providers,
            identity: engine_api::EmailAddress::new(user),
        }],
        None,
        observer,
    );
    app.install_account_ops(store.clone());
    (app, account_id, store)
}

/// The settings snapshot's row for `account`, the proofs' read fixture
/// over the product's own snapshot.
pub(super) fn row_of<'a>(
    snapshot: &'a astra_viewmodel::SettingsSnapshot,
    account: &AccountId,
) -> &'a astra_viewmodel::AccountSettingsRow {
    snapshot
        .accounts
        .iter()
        .find(|row| &row.account == account)
        .expect("the account renders a settings row")
}

/// The good save the failure and clear proofs stand up first: a section
/// naming both origins of the harness's shared listener with the login
/// and the password, the editor's freshly typed form.
pub(super) async fn save_both_legs(
    app: &App<Box<dyn Provider>>,
    account: &AccountId,
    base: &str,
    user: &str,
    password: &str,
) {
    app.dispatch(Intent::Settings(SettingsIntent::SetDav {
        account: account.clone(),
        section: Some(DavSectionInput {
            caldav_url: Some(base.to_owned()),
            carddav_url: Some(base.to_owned()),
            username: Some(user.to_owned()),
            password: Some(password.to_owned()),
        }),
    }))
    .await
    .unwrap();
}
