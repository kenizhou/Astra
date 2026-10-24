// SPDX-License-Identifier: GPL-3.0-only
//! The platform legs' host wiring: the OS zone the boot resolves and
//! reports, and the first product events' shapes.
//!
//! The zone judgement, recorded once: the host resolves the OS zone
//! once at boot and reports it through the core's device-zone arm,
//! where the validation, the once-per-value latch, and the
//! picked-zone marker all live. No while-running watch ships, because
//! the OS exposes the zone as a readable value but no change
//! notification a windowless host piece can ride (the Windows
//! broadcast reaches window procedures the shell's webview owns, and
//! a poll is a poll, not a notification). The core's latch accepts
//! any later report, so the web's own resolution (the design's
//! web-side arm) or a future watch drops in without shape changes.

use std::sync::Arc;

use crate::app_state::AppState;

/// The device's own zone, as the OS resolves it: the IANA id the
/// core's report arm validates.
pub(crate) fn resolved_zone() -> Option<String> {
    iana_time_zone::get_timezone().ok()
}

/// Reports `resolve`'s answer through the core's device-zone arm: the
/// core validates the id, consults its latches, and either publishes
/// the offer on the settings snapshot or publishes nothing. A
/// resolution that answers nothing reports nothing, and the boot
/// moves on: the offer is an offer, never a boot's errand.
pub(crate) fn report_zone_with(state: &AppState, resolve: impl FnOnce() -> Option<String>) {
    let Some(zone) = resolve() else {
        log::warn!("device zone: the OS zone could not be resolved; nothing reports");
        return;
    };
    let app = Arc::clone(&state.app);
    state.runtime.block_on(|| async move {
        if let Err(err) = app
            .dispatch(astra_app::Intent::Settings(
                astra_app::SettingsIntent::ReportDeviceZone { zone },
            ))
            .await
        {
            log::warn!("device zone: the report refused: {err}");
        }
    });
}

/// The boot report: the OS zone resolved once, at the moment the
/// windowed state stands. The offer the report may publish is on the
/// settings snapshot before the web's first pull.
pub(crate) fn report_boot_zone(state: &AppState) {
    report_zone_with(state, resolved_zone);
}

/// The boot's own product event: the app opened over `accounts`
/// standing accounts, the counts-and-shapes rule's first row.
pub(crate) fn app_open_event(accounts: usize) -> astra_app::AnalyticsEvent {
    astra_app::AnalyticsEvent::of("app_open").with_count("accounts", accounts as u64)
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
