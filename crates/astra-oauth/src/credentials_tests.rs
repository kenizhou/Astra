// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the compile-time registration holders: the blank-value rule
//! that keeps a CI build honest, written against the pure helpers the
//! holders read through.

use super::*;

#[test]
fn blank_and_unset_are_the_same_answer() {
    // A CI run without access to the registrations sets the variables to
    // the empty string rather than leaving them unset, and a build
    // carrying `client_id=""` would fail at the provider instead of at
    // the wizard.
    assert_eq!(present(None), None);
    assert_eq!(present(Some("")), None);
    assert_eq!(present(Some("   ")), None);
    assert_eq!(present(Some(" abc ")), Some("abc"));
}
