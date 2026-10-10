// SPDX-License-Identifier: GPL-3.0-only
//! `accounts list`: the account index's ids. The honest read for a CLI with
//! no credential access: the index file names what exists, the OS keyring
//! holds each config (a credential the CLI never opens), and the engine
//! store exposes no account enumeration of its own.

use std::path::Path;

use serde_json::json;

use super::Answer;
use crate::envelope::Code;

/// The account index's file name under the data dir, the same file the
/// desktop boot writes (`boot.rs`).
pub(crate) const ACCOUNT_INDEX_FILE: &str = "accounts.json";

/// The index's on-disk shape.
#[derive(serde::Deserialize)]
struct AccountIndex {
    /// The stored accounts' ids, in the order they were added.
    #[serde(default)]
    accounts: Vec<String>,
}

/// The ids the account index lists, in order; an absent index is an empty
/// list, not an error, exactly as the boot treats it.
pub(crate) fn index_ids(data_dir: &Path) -> Result<Vec<String>, String> {
    let path = data_dir.join(ACCOUNT_INDEX_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<AccountIndex>(&text)
            .map(|index| index.accounts)
            .map_err(|err| format!("the account index could not be read: {err}")),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(format!("the account index could not be read: {err}")),
    }
}

/// Lists the account index's ids.
pub(crate) fn list(data_dir: &Path) -> Answer {
    match index_ids(data_dir) {
        Ok(accounts) => Ok(json!({ "accounts": accounts })),
        Err(message) => Err((Code::Runtime, message)),
    }
}
