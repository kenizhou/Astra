// SPDX-License-Identifier: GPL-3.0-only
//! The composer's per-draft crypto toggles (ECS spec). The crypto send path
//! itself lives in the engine now: `submit_and_drain` stamps these flags on
//! the engine's [`Draft`](engine_api::Draft), and the account's installed
//! MIME wrapper (astra-crypto-store's, installed at dial) wraps: or refuses
//!: inside the engine's own outbox. There is deliberately no Astra-side
//! send machinery here anymore.

use serde::{Deserialize, Serialize};

/// The composer's per-draft crypto toggles on a submit intent. `None`
/// means "the account's policy default": with a crypto method configured
/// the default is to sign and encrypt (the account opted into crypto).
/// Stamped onto every draft the engine queues: inert on an account with no
/// wrapper installed (a plain account's send is untouched), live the moment
/// one is: including an op queued before the method was set and drained
/// after, which wraps. That semantic is the deliberate one: the flags say
/// what the user asked for, and the drain honours it against the account's
/// crypto as it stands when the send goes out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CryptoFlags {
    /// Sign the message; defaults to `true` under a crypto method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign: Option<bool>,
    /// Encrypt the message; defaults to `true` under a crypto method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypt: Option<bool>,
}

impl CryptoFlags {
    /// The resolved toggles.
    pub(crate) fn resolved(self) -> (bool, bool) {
        (self.sign.unwrap_or(true), self.encrypt.unwrap_or(true))
    }
}
