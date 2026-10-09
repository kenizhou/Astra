// SPDX-License-Identifier: GPL-3.0-only
//! The email-first setup step's host side: the DNS seam detection resolves through,
//! the bounded run that drives one detection, and the recommendation view that
//! crosses the IPC.
//!
//! The detection crate ships no DNS resolver on purpose: each platform's client
//! answers MX and SRV questions with its native API so the device's real DNS
//! settings apply. mail-calendar's clients reach GLib, JCA, or dnssd that way; a
//! Tauri Rust host has no platform DNS API in reach and std's `getaddrinfo`
//! answers neither MX nor SRV, so this host resolves through hickory over the
//! system's configured DNS servers (see the `hickory-resolver` entry in the
//! workspace table).
//!
//! The run is bounded to five seconds, tighter than the detection crate's own
//! ten second default: a person is watching the setup form, and the bound also
//! covers the post-run DAV probe the crate's internal deadline does not.

use std::{future::Future, sync::Arc, time::Duration};

use astra_autodetect::{
    DetectConfig, DetectError, Detected, MxError, MxRecord, MxResolution, MxResolver, SrvRecord,
    SrvResolution,
};
use hickory_resolver::{
    lookup::Lookup,
    proto::rr::{Name, RData, Record, RecordType},
    TokioResolver,
};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;

use crate::commands::CommandError;
use crate::runtime::AppRuntime;

/// The hard bound around one whole detection run: the crate's own deadline plus
/// the DAV probe that follows it, both within five seconds of the typed email.
pub(crate) const DETECT_BOUND: Duration = Duration::from_secs(5);

/// One MX or SRV lookup's own budget, matching the detection crate's `dns_timeout`
/// default; the resolver gives up on an unresponsive server at this point rather
/// than holding the run's worker thread.
const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(3);

/// The sign-in routes this build can offer: none yet, until an OAuth slice
/// carries a client registration. Detection then never routes to a browser
/// sign-in, and a Microsoft-hosted domain says so instead of prefilling a
/// password form its Basic auth retirement makes unusable.
const OAUTH_ROUTES: astra_account::OauthRoutes = astra_account::OauthRoutes {
    google: false,
    microsoft: false,
};

/// The production detection tuning: mail-calendar's defaults with the whole-run
/// deadline clamped from ten seconds to the host's five second bound, and the TLS
/// trust every account connection of this product already runs under.
pub(crate) fn detect_config() -> DetectConfig {
    DetectConfig {
        overall_deadline: DETECT_BOUND,
        tls_policy: astra_account::setup_trust_policy(),
        ..DetectConfig::default()
    }
}

/// The detection result as the setup form reads it: the routed recommendation,
/// the uses it offers, and what detection found before routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RecommendationView {
    /// Where the email-first step routes, with everything the target form needs
    /// prefilled.
    pub(crate) recommendation: astra_account::SetupRecommendation,
    /// The uses this route offers, in the order the form lists them.
    pub(crate) choices: Vec<astra_account::SetupChoice>,
    /// What the detection run itself found, before the recommendation routed it.
    pub(crate) detected: DetectedSummary,
}

/// What one detection run found, for the card's summary line: the outcome kind
/// and, when a strategy won, which one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct DetectedSummary {
    /// The outcome the run returned.
    pub(crate) outcome: DetectedOutcome,
    /// The strategy that produced the winning result, or `None` when nothing won.
    pub(crate) source_kind: Option<astra_autodetect::SourceKind>,
}

/// The outcome of one detection run, before routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DetectedOutcome {
    /// The domain speaks JMAP.
    Jmap,
    /// IMAP/SMTP settings were published for the domain.
    Mail,
    /// No mail server, but a calendar or address-book server was named.
    Dav,
    /// No strategy produced usable settings.
    Nothing {
        /// Whether every strategy failed on transport, likely offline.
        network_error: bool,
    },
}

/// Maps a detection run's result for `email` onto the view the setup form reads,
/// offering no browser sign-ins (this build carries no registration).
pub(crate) fn recommendation_view(email: &str, detected: Detected) -> RecommendationView {
    let summary = summary_of(&detected);
    let recommendation = astra_account::recommend(email, detected, OAUTH_ROUTES);
    let choices = recommendation.choices();
    RecommendationView {
        recommendation,
        choices,
        detected: summary,
    }
}

/// The summary half of the view: the outcome kind and the winning strategy.
fn summary_of(detected: &Detected) -> DetectedSummary {
    match detected {
        Detected::Jmap(jmap) => DetectedSummary {
            outcome: DetectedOutcome::Jmap,
            source_kind: Some(jmap.source.kind),
        },
        Detected::Mail(mail) => DetectedSummary {
            outcome: DetectedOutcome::Mail,
            source_kind: Some(mail.source.kind),
        },
        Detected::Dav(_) => DetectedSummary {
            outcome: DetectedOutcome::Dav,
            source_kind: None,
        },
        Detected::Nothing { network_error } => DetectedSummary {
            outcome: DetectedOutcome::Nothing {
                network_error: *network_error,
            },
            source_kind: None,
        },
    }
}

/// Why a bounded detection run produced no view.
#[derive(Debug)]
enum DetectFailure {
    /// The typed error detection itself returned before the run started.
    Detect(DetectError),
    /// The run did not finish within its bound.
    TimedOut,
}

/// Drives one detection run to completion under `bound`, then maps its result onto
/// the recommendation view. `detect` is the run itself, handed in as a closure so
/// the tests answer from a fixed result without touching the network; production
/// calls the detection crate with the host resolver. Every failure is a typed
/// error, never a panic.
///
/// # Errors
///
/// Returns [`CommandError::Detect`] when the address has no usable domain, the
/// trust store cannot be built, or the run outlives `bound`.
pub(crate) fn detect_settings_with<F, Fut>(
    runtime: &AppRuntime,
    email: &str,
    bound: Duration,
    detect: F,
) -> Result<RecommendationView, CommandError>
where
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = Result<Detected, DetectError>>,
{
    let run = runtime.block_on(|| async {
        match tokio::time::timeout(bound, detect()).await {
            Ok(result) => result.map_err(DetectFailure::Detect),
            Err(_elapsed) => Err(DetectFailure::TimedOut),
        }
    });
    match run {
        Ok(detected) => Ok(recommendation_view(email, detected)),
        Err(DetectFailure::Detect(DetectError::InvalidEmail)) => Err(CommandError::Detect(
            "the email address has no usable domain".to_owned(),
        )),
        Err(DetectFailure::Detect(DetectError::Tls(reason))) => Err(CommandError::Detect(format!(
            "the trust store for detection could not be built: {reason}"
        ))),
        Err(DetectFailure::TimedOut) => Err(CommandError::Detect(format!(
            "detection timed out after {bound:?}"
        ))),
    }
}

/// The host's DNS seam: MX and SRV lookups over the system's configured DNS
/// servers, driven on the app runtime. Built per detection run; when the system's
/// DNS configuration cannot be read, [`DetectionResolver::new`] answers `None`
/// and detection runs without the MX fallback and the SRV strategies.
pub(crate) struct DetectionResolver {
    /// The resolver the lookups run through.
    resolver: TokioResolver,
    /// The app runtime's handle, whose workers drive the resolver's I/O.
    handle: Handle,
}

impl DetectionResolver {
    /// Builds the resolver from the system's DNS configuration, with one lookup's
    /// budget set so an unresponsive server cannot hold a worker thread past the
    /// detection crate's own `dns_timeout` bound.
    pub(crate) fn new(handle: Handle) -> Option<Self> {
        let mut builder = TokioResolver::builder_tokio().ok()?;
        let options = builder.options_mut();
        options.timeout = DNS_LOOKUP_TIMEOUT;
        options.attempts = 1;
        let resolver = builder.build().ok()?;
        Some(Self { resolver, handle })
    }

    /// One lookup, driven on the app runtime from the calling worker thread. The
    /// detection crate calls the seam on a blocking thread; `drive` is the
    /// sanctioned way this host waits on a future from one.
    fn lookup(&self, name: &str, record_type: RecordType) -> Result<Lookup, MxError> {
        let name = Name::parse(name, None).map_err(|err| MxError::Lookup(err.to_string()))?;
        let resolver = self.resolver.clone();
        crate::runtime::drive(&self.handle, move || async move {
            resolver.lookup(name, record_type).await
        })
        .map_err(|err| MxError::Lookup(err.to_string()))
    }
}

impl MxResolver for DetectionResolver {
    fn resolve_mx(&self, domain: &str) -> Result<MxResolution, MxError> {
        // The AD bit is not surfaced by this resolver; it takes part in no trust
        // decision today (MX-derived results are trusted on the HTTPS fetch).
        let answers = self.lookup(domain, RecordType::MX)?;
        Ok(MxResolution {
            records: answers.answers().iter().filter_map(mx_record).collect(),
            authentic_data: false,
        })
    }

    fn resolve_srv(&self, name: &str) -> Result<SrvResolution, MxError> {
        let answers = self.lookup(name, RecordType::SRV)?;
        Ok(SrvResolution {
            records: answers.answers().iter().filter_map(srv_record).collect(),
            authentic_data: false,
        })
    }
}

/// Maps one MX answer record onto the detection crate's shape, `None` for any
/// other record type the answer carried.
fn mx_record(record: &Record) -> Option<MxRecord> {
    let RData::MX(mx) = &record.data else {
        return None;
    };
    Some(MxRecord {
        preference: mx.preference,
        exchange: trim_dns_name(&mx.exchange.to_string()),
    })
}

/// Maps one SRV answer record onto the detection crate's shape, `None` for any
/// other record type the answer carried.
fn srv_record(record: &Record) -> Option<SrvRecord> {
    let RData::SRV(srv) = &record.data else {
        return None;
    };
    Some(SrvRecord {
        priority: srv.priority,
        weight: srv.weight,
        port: srv.port,
        target: trim_dns_name(&srv.target.to_string()),
    })
}

/// A DNS name as it crossed the wire, minus the root dot: a bare name parses as a
/// domain downstream, while the RFC 2782 `.` sentinel stays recognisable.
fn trim_dns_name(name: &str) -> String {
    if name == "." {
        name.to_owned()
    } else {
        name.strip_suffix('.').unwrap_or(name).to_owned()
    }
}

/// The resolver the command passes to a detection run, or `None` when the system's
/// DNS configuration could not be read; the run then skips the DNS-derived
/// strategies and says so in the diagnostic log.
pub(crate) fn detection_resolver(handle: Handle) -> Option<Arc<dyn MxResolver>> {
    let resolver =
        DetectionResolver::new(handle).map(|resolver| Arc::new(resolver) as Arc<dyn MxResolver>);
    if resolver.is_none() {
        log::warn!("detect: no usable system DNS configuration; MX and SRV lookups skipped");
    }
    resolver
}

#[cfg(test)]
#[path = "detection_tests.rs"]
mod tests;
