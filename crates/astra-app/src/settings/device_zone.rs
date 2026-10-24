// SPDX-License-Identifier: GPL-3.0-only
//! The zone family: the zone write both zone intents share, and the
//! device-zone offer's report, acceptance, and dismissal.
//!
//! The decisions this module owns, recorded once:
//!
//! - **A picked zone stands, and the offer is not even made.** The pick is
//!   its own persisted marker ([`zone_picked` on
//!   `Preferences`](crate::Preferences)), set by the zone picker's write
//!   and never by the offer's acceptance, because the standing zone value
//!   cannot tell an explicit `Etc/UTC` pick from an untouched default and
//!   the pin deserves better than that inference. A report against a
//!   picked zone publishes no offer; an acceptance against one refuses
//!   typed, and the picked zone stands.
//! - **The offer is once per zone value per process.** The latch is the
//!   per-process set of already offered values, not a per-change counter:
//!   a value that returns (a flapping device zone) was already offered
//!   and does not nag again, while a genuinely new value (travel) offers.
//!   The persisted `zone_offered` bool is the first-run latch: once any
//!   offer has ever been made, a later process's first report (the boot
//!   resolution) never offers again, and its reported value counts as
//!   already offered so a host's repeated boot reports never nag either.
//! - **The pending offer is the unanswered question.** A standing report
//!   replaces it; the accept, a pick, and the dismissal clear it; a
//!   refused report leaves it as it stood, because a refusal does not
//!   answer the question the surface is still asking.
//!
//! [`zone_picked` on `Preferences`]: crate::Preferences

use astra_viewmodel::SettingsError;
use engine_api::{AccountId, Provider};

use crate::App;

impl<P: Provider> App<P> {
    /// The zone write both zone intents share: the id must resolve through
    /// the engine's zone resolver, because every floating and all-day
    /// instant in the calendar resolves through the display zone and a
    /// zone the engine cannot localise is a diary that cannot be laid out.
    /// A refusal keeps the standing zone. The zone and its picked marker
    /// move as one write, so a pick never lands without its marker and an
    /// acceptance never clears one: the acceptance is refused outright
    /// while a pick stands, and the refusal is re-checked inside the
    /// write itself, because the acceptance's own pre-check spans two
    /// lock acquisitions and a pick landing between them must still win.
    /// Returns whether the write happened.
    pub(crate) async fn set_zone(&self, zone: String, picked: bool) -> bool {
        let resolvable =
            engine_api::TimeZoneId::iana(&zone).is_ok_and(|id| engine_api::is_supported_zone(&id));
        if !resolvable {
            self.answer(Some(SettingsError::ZoneUnresolvable)).await;
            return false;
        }
        // The check and the act in one acquisition: an unpicked write
        // (the offer's acceptance) re-reads the picked marker under the
        // lock it writes through, so a pick that raced the acceptance's
        // pre-check still stands after it.
        let (refused, save_failed) = {
            let mut pane = self.pane.lock().expect("preferences mutex poisoned");
            if !picked && pane.prefs.zone_picked {
                (true, false)
            } else {
                (
                    false,
                    pane.set_setting(|prefs| {
                        prefs.zone = zone;
                        prefs.zone_picked = picked;
                    }),
                )
            }
        };
        if refused {
            self.answer(Some(SettingsError::ZonePicked)).await;
            return false;
        }
        self.answer(save_failed.then_some(SettingsError::PersistFailed))
            .await;
        // The calendar resolves through the zone, so it republishes beside
        // the settings block: the same change is a change to both surfaces.
        // The re-expansion comes first, because a floating event's stored
        // instant is only correct for the zone it was expanded under; the
        // accounts whose re-expansion refused keep old-zone instants, so
        // their rows say a retry is owed while the zone itself stands.
        let stranded = self.reexpand_calendar().await;
        self.answer_zone_reexpansion(&stranded).await;
        self.rebuild_calendar().await;
        true
    }

    /// Records the zone change's per-account re-expansion outcomes and
    /// republishes: the accounts whose re-expansion refused carry the
    /// sentence, the rest clear, so the channel states the newest zone
    /// change the way the others do. The zone itself stands, because it
    /// persisted; this names the accounts whose stored occurrences still
    /// resolve through the old one.
    pub(crate) async fn answer_zone_reexpansion(&self, stranded: &[AccountId]) {
        {
            let mut state = self.settings_state.lock().expect("settings mutex poisoned");
            state.refresh_errors.clear();
            for account in stranded {
                state.refresh_errors.insert(
                    account.clone(),
                    "the diary could not be re-expanded under the new zone; set the zone again to retry"
                        .to_owned(),
                );
            }
        }
        self.rebuild_settings().await;
    }

    /// The device-zone report: the host's answer to a resolution or a
    /// change it observed. The core validates the id, consults the picked
    /// marker and the latches, and either publishes the standing offer on
    /// the settings snapshot or publishes nothing. A refused report
    /// latches nothing and leaves any standing offer as it stood: a
    /// refusal does not answer the question the surface is still asking.
    pub(crate) async fn report_device_zone(&self, zone: String) {
        let resolvable =
            engine_api::TimeZoneId::iana(&zone).is_ok_and(|id| engine_api::is_supported_zone(&id));
        let (picked, latched) = {
            let pane = self.pane.lock().expect("preferences mutex poisoned");
            (pane.prefs.zone_picked, pane.prefs.zone_offered)
        };
        let mut stands = false;
        {
            let mut state = self.settings_state.lock().expect("settings mutex poisoned");
            if !resolvable || picked || state.offered_device_zones.contains(&zone) {
                // Refused: nothing latches, and the pending offer stands
                // as it stood.
            } else if state.offered_device_zones.is_empty() && latched {
                // An earlier process's offer consumed the once-latch, so
                // this process's first reported value never re-offers; it
                // counts as already offered, so a host's repeated boot
                // reports never nag either.
                state.offered_device_zones.insert(zone);
            } else {
                state.offered_device_zones.insert(zone.clone());
                state.device_zone_offer = Some(zone);
                stands = true;
            }
        }
        if stands {
            // The latch persists through the shared tail, but a report is
            // the host's word, not a person's write, so its failure logs
            // rather than occupying the settings error channel: the cost
            // is a relaunch that may offer again, which the surface never
            // needs to say.
            let save_failed = self
                .pane
                .lock()
                .expect("preferences mutex poisoned")
                .set_setting(|prefs| prefs.zone_offered = true);
            if save_failed {
                log::warn!(
                    "device zone: the offer's once-latch did not persist; a relaunch may offer again"
                );
            }
        }
        self.rebuild_settings().await;
    }

    /// The device-zone offer's acceptance: refused, typed, when a zone was
    /// picked, otherwise written through the same path the zone picker
    /// rides. The acceptance is not a pick: the marker never moves here,
    /// so a later device-zone change may still offer. The answered offer
    /// clears only when the write landed, so a refused acceptance leaves
    /// the question the surface is still asking standing.
    pub(crate) async fn accept_device_zone(&self, zone: String) {
        let picked = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .prefs
            .zone_picked;
        if picked {
            self.answer(Some(SettingsError::ZonePicked)).await;
            return;
        }
        if self.set_zone(zone, false).await {
            self.clear_device_zone_offer().await;
        }
    }

    /// The device-zone offer's dismissal: the pending offer clears and the
    /// latches stand, so the dismissed value never offers again this
    /// process and the first-run offer never returns.
    pub(crate) async fn dismiss_device_zone(&self) {
        self.clear_device_zone_offer().await;
    }

    /// Clears the standing device-zone offer, republishing when one stood:
    /// the accept, a pick, and the dismissal all answer the pending
    /// question, and a surface reading the snapshot after any of them sees
    /// no offer standing.
    pub(crate) async fn clear_device_zone_offer(&self) {
        let cleared = self
            .settings_state
            .lock()
            .expect("settings mutex poisoned")
            .device_zone_offer
            .take()
            .is_some();
        if cleared {
            self.rebuild_settings().await;
        }
    }
}
