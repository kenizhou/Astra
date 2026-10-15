// SPDX-License-Identifier: GPL-3.0-only
//! The Microsoft Graph half of an account: the token source the
//! account's providers dial through.

mod token_source;

pub use token_source::{CredentialOrigin, GraphTokenSource, TokenSink};
