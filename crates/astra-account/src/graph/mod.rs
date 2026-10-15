// SPDX-License-Identifier: GPL-3.0-only
//! The Microsoft Graph half of an account: the token source the
//! account's providers dial through, the refreshing providers that wrap
//! the engine's Graph adapters over it, and the dial that assembles a
//! Microsoft account's providers.

mod address;
mod calendar;
mod dial;
mod provider;
// `pub(crate)`, not `mod`: the google module's tests reuse the mock
// token endpoints this module's own tests drive (the ratcheting server
// that models a rotating grant is one contract, not two), and the
// module's own items stay private to it.
pub(crate) mod token_source;

pub use address::{fetch_primary_address, fetch_primary_address_at};
pub use calendar::connect_graph_calendar_providers;
pub use dial::{
    connect_graph_folder, connect_graph_mail_providers, dial_microsoft_account, MicrosoftProviders,
};
pub use token_source::{CredentialOrigin, GraphTokenSource, TokenSink};
