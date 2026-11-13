// SPDX-License-Identifier: GPL-3.0-only
//! The blocked-sender preference's write half: the address normalisation the
//! blocklist keys on and the block/unblock moves the settings arms ride.
//! Split from the document half so neither file grows past the cap.
//!
//! Suppression only: a blocked sender's mail raises no new-mail notification,
//! but it still lands in the Inbox. Filing mail as junk stays the Junk verb's
//! job; the blocklist never moves a message.

use super::Preferences;

/// The bare addr-spec a sender address names: trimmed and lowercased, and
/// lifted out of a display form ("Ada <ada@x.com>") when one arrives, so
/// every spelling of one sender agrees on one key. A blank address reads as
/// empty. The VIP set's normaliseAddress idiom, mirrored core-side.
pub(crate) fn normalise_sender_address(address: &str) -> String {
    let lowered = address.trim().to_lowercase();
    // The first angle-bracket group whose inside carries no further '<',
    // the display form's addr-spec; the whole lowered string otherwise.
    let spec = lowered
        .find('<')
        .and_then(|open| {
            let rest = &lowered[open + 1..];
            let close = rest.find('>')?;
            let inner = &rest[..close];
            (!inner.contains('<')).then_some(inner)
        })
        .unwrap_or(lowered.as_str());
    spec.trim().to_owned()
}

impl Preferences {
    /// Blocks `address`: its normalised bare addr-spec joins the blocklist.
    /// A blank address stores nothing.
    pub fn block_sender(&mut self, address: &str) {
        let key = normalise_sender_address(address);
        if key.is_empty() {
            return;
        }
        self.blocked_senders.insert(key);
    }

    /// Unblocks `address`: its normalised bare addr-spec leaves the
    /// blocklist. A blank address removes nothing.
    pub fn unblock_sender(&mut self, address: &str) {
        let key = normalise_sender_address(address);
        if key.is_empty() {
            return;
        }
        self.blocked_senders.remove(&key);
    }
}

#[cfg(test)]
#[path = "../blocking_tests.rs"]
mod tests;
