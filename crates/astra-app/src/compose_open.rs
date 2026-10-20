// SPDX-License-Identifier: GPL-3.0-only
//! The composition a `mailto:` link opens: the core parser consumed at the
//! app boundary, its five honoured fields carried as one additive prefill
//! under the account the default send account resolves to. Request-free by
//! construction: the composition sends as plain mail exactly as a blank new
//! message does, because a link answers no message however it is spelled.

use astra_viewmodel::MailtoOpen;
use engine_api::{AccountId, Provider};

use crate::App;

// The prefill `uri` opens, from the account the caller resolved as the
// default send account. Every header the parser does not honour, `from`
// above all, is dropped before this sees the link; a uri that is not a
// `mailto:` link at all answers `None`, so a caller never opens a composer
// over a link it could not read.
#[must_use]
pub(crate) fn mailto_open(uri: &str, from: AccountId) -> Option<MailtoOpen> {
    astra_composer::parse_mailto(uri).map(|parsed| MailtoOpen {
        from,
        to: parsed.to,
        cc: parsed.cc,
        bcc: parsed.bcc,
        subject: parsed.subject,
        body: parsed.body,
    })
}

impl<P: Provider> App<P> {
    /// The composition a `mailto:` link opens: the parser's five fields under
    /// the default send account, the persisted preference when it names a
    /// standing account and the first account otherwise. The host leg's one
    /// call: parse, resolve, hand the prefill to the composer.
    pub async fn open_mailto(&self, uri: &str) -> Option<MailtoOpen> {
        let accounts: Vec<AccountId> = self
            .account_handles()
            .await
            .iter()
            .map(|account| account.id.clone())
            .collect();
        let from = self
            .pane
            .lock()
            .expect("preferences mutex poisoned")
            .send_account(&accounts)?;
        mailto_open(uri, from)
    }
}

#[cfg(test)]
#[path = "compose_open_tests.rs"]
mod tests;
