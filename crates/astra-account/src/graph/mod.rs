// SPDX-License-Identifier: GPL-3.0-only
//! The Microsoft Graph half of an account: the token source today, the
//! provider dials the later slices build over it.

mod token_source;

pub use token_source::{CredentialOrigin, GraphTokenSource, TokenSink};
