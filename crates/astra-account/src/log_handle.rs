// SPDX-License-Identifier: GPL-3.0-only
//! A short, stable, non-identifying handle for naming *which account* a
//! log line is about.
//!
//! # Why a log line needs one at all
//!
//! OAuth lines without one are unattributed: three identical "the graph
//! server renewed this account's sign-in" lines on a six-account device
//! distinguish neither *three accounts renewing once* from *one account
//! renewing three times*, which are a healthy launch and a refresh loop
//! respectively. A token refresh happens on a provider's own task with no
//! account list in scope, so a position in a list is not available either,
//! and inventing a second numbering would make `account[2]` mean two
//! different accounts depending on which line it is read from.
//!
//! # Why not the account id
//!
//! An account id is `<address>@<host>`, so it is an address and an
//! endpoint, and a diagnostic log is meant to be safe to attach to a
//! support request. The handle below reveals neither.
//!
//! # What this is
//!
//! Four hex characters of an FNV-1a digest of the account id: enough to
//! tell a handful of accounts apart at a glance, and deliberately not
//! enough to be a durable identity. The input is not recoverable from it,
//! and it is only meaningful *within* a log, where it answers the one
//! question asked of it: same account, or a different one? It is also
//! stable across launches, so "this same account has rotated at every
//! launch this week" is readable from a week of logs.
//!
//! FNV-1a is written out here rather than taken from `DefaultHasher` on
//! purpose: `DefaultHasher`'s output is explicitly not guaranteed stable
//! across Rust releases, which would silently break the across-launches
//! property the next time the toolchain pin moves.

/// A short, stable, non-identifying handle for `account_id`, for use in a
/// log line.
///
/// Formatted as `acct:ab12`. See this module's docs for why it is a digest
/// rather than the id or a position: it is neither an address, an endpoint,
/// nor a credential, and it cannot be turned back into one.
#[must_use]
pub fn account_log_handle(account_id: &str) -> String {
    // FNV-1a, 32-bit. Fixed constants, so the mapping is the same in every build, forever.
    const OFFSET_BASIS: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let mut hash = OFFSET_BASIS;
    for byte in account_id.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    // The low 16 bits: 65,536 buckets over the handful of accounts one device holds. A collision
    // would merely make two accounts read alike in one log, never mix up any *state*.
    format!("acct:{:04x}", hash & 0xffff)
}

#[cfg(test)]
mod tests {
    use super::account_log_handle;

    /// The property the whole point rests on: the same account reads the
    /// same everywhere, so a reader can follow one account through a log,
    /// and across a week of them.
    #[test]
    fn the_same_account_always_reads_the_same() {
        assert_eq!(
            account_log_handle("alice@example.com@graph.microsoft.com"),
            account_log_handle("alice@example.com@graph.microsoft.com"),
        );
    }

    /// The other half: accounts that differ must read differently, or the
    /// handle answers the question wrongly rather than not at all. Checked
    /// over a realistic set: the same person's address at two providers,
    /// and two people at one; since those are exactly the pairs a support
    /// log puts side by side.
    #[test]
    fn accounts_that_differ_read_differently() {
        let handles: Vec<String> = [
            "alice@example.com@graph.microsoft.com",
            "alice@example.com@mail.google.com",
            "bob@example.com@graph.microsoft.com",
            "alice@example.org@graph.microsoft.com",
            "alice@example.com@imap.example.com",
        ]
        .into_iter()
        .map(account_log_handle)
        .collect();
        let mut unique = handles.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), handles.len(), "handles collided: {handles:?}");
    }

    /// The privacy rule, as a check rather than an intention: nothing of
    /// the address survives into the handle. Asserted on the *pieces*: a
    /// four-hex-character string cannot contain them, but that is only
    /// true for as long as the format stays four hex characters, and this
    /// is what would fail if someone "improved" it by appending the domain
    /// for readability.
    #[test]
    fn the_handle_carries_no_part_of_the_address() {
        let handle = account_log_handle("alice@example.com@imap.fastmail.com");
        for fragment in ["alice", "example", "fastmail", "com", "@", "."] {
            assert!(
                !handle.trim_start_matches("acct:").contains(fragment),
                "{fragment:?} reached the log handle {handle:?}",
            );
        }
        assert_eq!(handle.len(), "acct:".len() + 4, "four hex characters");
        assert!(handle
            .trim_start_matches("acct:")
            .chars()
            .all(|c| c.is_ascii_hexdigit()),);
    }

    /// Stability is a *value* property, not just a determinism one: these
    /// are the handles this function produces, and they must survive a
    /// refactor of its internals. Without this, a change of hash would keep
    /// every other test green while silently breaking the ability to follow
    /// one account across two launches: the thing the digest exists for.
    #[test]
    fn the_handles_are_the_ones_this_function_has_always_produced() {
        // The FNV-1a offset basis itself (0x811c_9dc5), and the standard `"a"` vector
        // (0xe40c_292c); both truncated to their low 16 bits.
        assert_eq!(account_log_handle(""), "acct:9dc5");
        assert_eq!(account_log_handle("a"), "acct:292c");
    }
}
