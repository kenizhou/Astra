// SPDX-License-Identifier: GPL-3.0-only
//! The lifecycle proof's scripted dial seams, split from the suite for
//! the 500-line cap: the minting dial that refreshes through the real
//! source (recording what the entry carried before and after, so the
//! rotation's mid-dial landing is observable) and the boot's one-answer
//! stand-in.

use std::sync::{Arc, Mutex};

use crate::dial_table::AccountDials;
use crate::oauth::google_add::GoogleDial;

use super::super::FakeDav;

/// What one lifecycle dial observed: the grant the registered entry
/// carried when the dial began, the access token the mint answered, and
/// the grant the entry carried after it.
#[derive(Default)]
pub(super) struct DialSaw {
    pub(super) entry_before: Mutex<Vec<String>>,
    pub(super) minted: Mutex<Vec<String>>,
    pub(super) entry_after: Mutex<Vec<String>>,
}

/// The dial seam that mints through the real source, exactly what the
/// dial's first provider call does: the server has moved past the access
/// token the exchange issued, so the mint refreshes against the scripted
/// endpoint and any rotation the answer carries lands through the real
/// sink while the dial is still running. Google does not rotate in
/// practice; this is the backstop path proven end to end.
pub(super) fn minting_dial(
    saw: Arc<DialSaw>,
    dials: Arc<AccountDials>,
    seeded_access: &'static str,
) -> GoogleDial {
    Arc::new(move |account, tokens, config| {
        let before = config
            .google
            .as_ref()
            .map(|google| google.refresh_token.expose().to_owned())
            .unwrap_or_default();
        let account = account.as_str().to_owned();
        let saw = Arc::clone(&saw);
        let dials = Arc::clone(&dials);
        Box::pin(async move {
            saw.entry_before.lock().unwrap().push(before);
            let minted = tokens.access_token_replacing(seeded_access).await?;
            saw.minted.lock().unwrap().push(minted);
            let after = dials
                .of(&account)
                .and_then(|(stored, _)| {
                    stored
                        .google
                        .map(|google| google.refresh_token.expose().to_owned())
                })
                .unwrap_or_default();
            saw.entry_after.lock().unwrap().push(after);
            Ok(astra_account::GoogleProviders {
                mail: vec![FakeDav::mail()],
                calendar: Some(FakeDav::calendar()),
                calendar_error: None,
            })
        })
    })
}

/// What the boot's dial seam answers.
#[derive(Clone, Copy)]
pub(super) enum BootAnswer {
    /// A dialled account with its account-global mail and its calendar.
    Healthy,
    /// A grant the token endpoint refused.
    DeadGrant,
}

/// The boot's dial seam over one scripted answer for a grant it records,
/// the healthy re-dial's stand-in: the real boot machinery builds the
/// source and reads the stored document, and the seam answers what the
/// account-global read would.
pub(super) fn boot_dial(grants: Arc<Mutex<Vec<String>>>, answer: BootAnswer) -> GoogleDial {
    Arc::new(move |_account, _tokens, config| {
        let grant = config
            .google
            .as_ref()
            .map(|google| google.refresh_token.expose().to_owned())
            .unwrap_or_default();
        let grants = Arc::clone(&grants);
        Box::pin(async move {
            grants.lock().unwrap().push(grant);
            match answer {
                BootAnswer::Healthy => Ok(astra_account::GoogleProviders {
                    mail: vec![FakeDav::mail()],
                    calendar: Some(FakeDav::calendar()),
                    calendar_error: None,
                }),
                // The dead word the real token source answers for a grant
                // the server refused, proven over HTTP at the token
                // source; a stored document's endpoint is pinned to the
                // real Google authority, so the boot leg scripts the
                // arrival.
                BootAnswer::DeadGrant => Err(astra_account::AccountError::SigninRejected(
                    "the refresh token was refused".to_owned(),
                )),
            }
        })
    })
}
