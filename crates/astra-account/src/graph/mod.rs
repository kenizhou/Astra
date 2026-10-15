// SPDX-License-Identifier: GPL-3.0-only
//! The Microsoft Graph half of an account: the token source the
//! account's providers dial through, the refreshing providers that wrap
//! the engine's Graph adapters over it, and the dial that assembles a
//! Microsoft account's providers.

mod address;
mod calendar;
mod dial;
mod provider;
mod token_source;

pub use address::fetch_primary_address;
pub use calendar::connect_graph_calendar_providers;
pub use dial::{
    connect_graph_folder, connect_graph_mail_providers, dial_microsoft_account, MicrosoftProviders,
};
pub use token_source::{CredentialOrigin, GraphTokenSource, TokenSink};
