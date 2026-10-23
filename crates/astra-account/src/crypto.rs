// SPDX-License-Identifier: GPL-3.0-only
//! The account's built-in crypto section of the stored config.

use serde::Deserialize;

/// The account's built-in crypto section: the send gate's method
/// (`openpgp` / `smime`; anything else reads as no crypto).
#[derive(Debug, Clone, Deserialize)]
pub struct CryptoAccount {
    /// The crypto method (`openpgp` / `smime`).
    pub method: String,
}
