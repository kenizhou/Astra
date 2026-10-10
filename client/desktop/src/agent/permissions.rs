// SPDX-License-Identifier: GPL-3.0-only
//! The permission bridge: the host side of the sidecar's fail-closed gate.
//! A `permission_request` envelope registers a pending approval the UI
//! answers through `permission_response`; every settle path (the answer, a
//! fail close on death or shutdown, or a duplicate ask) is audited into
//! the ledger with the turn's correlation id, and an answer for an unknown
//! or consumed id is the typed error the sidecar's own bridge mirrors.

use std::collections::HashMap;
use std::sync::Mutex;

use astra_ai::{AiStore, AuditRow, CorrelationId};

use super::protocol::PermissionLevel;

/// One approval awaiting the user's decision, as the event carries it to
/// the UI and the answer names it back.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct PendingApproval {
    /// The request id the answer names.
    pub(crate) id: String,
    /// The tool call waiting on the decision.
    pub(crate) tool_call_id: String,
    /// The tool's name.
    pub(crate) tool: String,
    /// The gate level the tool ran under.
    pub(crate) level: PermissionLevel,
    /// The tool's arguments.
    pub(crate) args: serde_json::Value,
    /// Renderable preview payload, when the tool produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) preview: Option<serde_json::Value>,
    /// The turn's correlation id, when the ask belongs to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) correlation: Option<String>,
}

/// How a pending approval settled, for the audit row's outcome.
#[derive(Debug, Clone, Copy)]
enum Settle {
    /// The user approved it.
    Approved,
    /// The user refused it.
    Rejected,
    /// It closed without reaching a user: death, shutdown, or an abort.
    Closed,
}

impl Settle {
    fn outcome(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Closed => "fail-closed",
        }
    }
}

/// The pending-approval table. One standard mutex: the guards are short
/// map operations, never held across an await.
pub(crate) struct PermissionBridge {
    /// The approvals awaiting a decision, by request id.
    pending: Mutex<HashMap<String, PendingApproval>>,
    /// The ledger the decisions land in.
    store: std::sync::Arc<AiStore>,
}

impl std::fmt::Debug for PermissionBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionBridge")
            .field(
                "pending",
                &self.pending.lock().expect("bridge mutex poisoned").len(),
            )
            .finish_non_exhaustive()
    }
}

/// The cap on the argument preview the audit row keeps: a final guard over
/// the shaped preview's ids and counts, never the mail it touched.
const PREVIEW_CAP: usize = 200;

impl PermissionBridge {
    /// An empty bridge over the ledger store.
    pub(crate) fn new(store: std::sync::Arc<AiStore>) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            store,
        }
    }

    /// Registers a freshly arrived ask. A duplicate id is a sidecar
    /// protocol bug and is refused: the ask is not overwritten and the
    /// caller leaves the sidecar's tool call blocked until a fail close.
    pub(crate) fn register(&self, approval: PendingApproval) -> Result<(), String> {
        let mut pending = self.pending.lock().expect("bridge mutex poisoned");
        if pending.contains_key(&approval.id) {
            return Err(format!("permission id already pending: {}", approval.id));
        }
        pending.insert(approval.id.clone(), approval);
        Ok(())
    }

    /// The approvals awaiting a decision, in ask order. Test-only here:
    /// the relay's `pending_approvals` gates it the same way, and the UI
    /// consumes the ask events instead until a pull command ships.
    #[cfg(test)]
    pub(crate) fn pending(&self) -> Vec<PendingApproval> {
        self.pending
            .lock()
            .expect("bridge mutex poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// Consumes the pending entry for `id`, audits the decision, and hands
    /// the consumed approval back: the response envelope still needs it
    /// for the turn's correlation id.
    ///
    /// # Errors
    /// The typed `permission-unknown-or-consumed:` error when nothing is
    /// pending under `id` (never answered, already answered, or fail
    /// closed).
    pub(crate) fn respond(
        &self,
        id: &str,
        approved: bool,
        always_allow: bool,
    ) -> Result<PendingApproval, String> {
        let approval = self.take(id)?;
        self.audit(
            &approval,
            if approved {
                Settle::Approved
            } else {
                Settle::Rejected
            },
            if always_allow { "always-allow" } else { "" },
        );
        Ok(approval)
    }

    /// Puts a consumed approval back: the response envelope failed to
    /// reach the sidecar, so the user's decision must not be lost.
    pub(crate) fn restore(&self, approval: PendingApproval) {
        self.pending
            .lock()
            .expect("bridge mutex poisoned")
            .insert(approval.id.clone(), approval);
    }

    /// Consumes every pending entry and audits each as fail-closed: the
    /// asks belong to a sidecar that is gone or going, and an answer for
    /// them must never be writable against the next one.
    pub(crate) fn fail_close_all(&self, reason: &str) -> Vec<PendingApproval> {
        let drained: Vec<PendingApproval> = {
            let mut pending = self.pending.lock().expect("bridge mutex poisoned");
            pending.drain().map(|(_, approval)| approval).collect()
        };
        for approval in &drained {
            self.audit(approval, Settle::Closed, reason);
        }
        drained
    }

    /// Takes one entry out of the map.
    fn take(&self, id: &str) -> Result<PendingApproval, String> {
        self.pending
            .lock()
            .expect("bridge mutex poisoned")
            .remove(id)
            .ok_or_else(|| format!("permission-unknown-or-consumed: {id}"))
    }

    /// Writes the decision's audit row. The correlation id is the ask's
    /// own when it carried one, else a fresh id: the row always lands,
    /// never dropped for a missing turn.
    fn audit(&self, approval: &PendingApproval, settle: Settle, detail: &str) {
        let correlation = approval
            .correlation
            .clone()
            .map_or_else(CorrelationId::generate, CorrelationId::new);
        let mut arg_preview = shaped_arg_preview(&approval.tool, &approval.args);
        if arg_preview.chars().count() > PREVIEW_CAP {
            arg_preview = arg_preview.chars().take(PREVIEW_CAP).collect();
        }
        if !detail.is_empty() {
            let _ = std::fmt::Write::write_fmt(&mut arg_preview, format_args!(" ({detail})"));
        }
        let row = AuditRow {
            correlation,
            verb: format!("permission {}", approval.tool),
            arg_preview,
            outcome: settle.outcome().to_string(),
        };
        if let Err(e) = self.store.record_audit(&row) {
            log::warn!("agent: the permission audit row failed: {e}");
        }
    }
}

/// The redacted argument preview for a permission ask, mirroring the CLI's
/// preview vocabulary: shapes (counts, lengths) and store ids only, never
/// addresses, subjects, or body text, so the two writers' rows in one
/// ledger read alike. An unknown tool gets the shape alone.
fn shaped_arg_preview(tool: &str, args: &serde_json::Value) -> String {
    let fields = args.as_object();
    let id = |key: &str| {
        fields
            .and_then(|fields| fields.get(key))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?")
    };
    let count = |key: &str| {
        fields
            .and_then(|fields| fields.get(key))
            .map_or(0, |value| match value {
                serde_json::Value::Array(items) => items.len(),
                serde_json::Value::String(_) => 1,
                _ => 0,
            })
    };
    let chars = |key: &str| {
        fields
            .and_then(|fields| fields.get(key))
            .and_then(serde_json::Value::as_str)
            .map_or(0, |text| text.chars().count())
    };
    match tool {
        "list_folders" => format!("account={}", id("account")),
        "list_messages" => {
            let mut preview = format!("account={}", id("account"));
            if let Some(folder) = fields
                .and_then(|fields| fields.get("folder"))
                .and_then(serde_json::Value::as_str)
            {
                let _ = std::fmt::Write::write_fmt(&mut preview, format_args!(" folder={folder}"));
            }
            if let Some(limit) = fields.and_then(|fields| fields.get("limit")) {
                let _ = std::fmt::Write::write_fmt(&mut preview, format_args!(" limit={limit}"));
            }
            preview
        }
        "read_message" => format!("account={} keys={}", id("account"), count("keys")),
        "save_draft" => format!(
            "account={} from={} to={} cc={} subject_chars={} body_chars={}",
            id("account"),
            count("from"),
            count("to"),
            count("cc"),
            chars("subject"),
            chars("body"),
        ),
        "discard_draft" => format!("account={} key={}", id("account"), id("key")),
        _ => format!("fields={}", fields.map_or(0, serde_json::Map::len)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval(id: &str, correlation: Option<&str>) -> PendingApproval {
        PendingApproval {
            id: id.to_string(),
            tool_call_id: "call-1".to_string(),
            tool: "save_draft".to_string(),
            level: PermissionLevel::Confirm,
            args: serde_json::json!({ "account": "acct-1" }),
            preview: None,
            correlation: correlation.map(str::to_string),
        }
    }

    fn bridge() -> PermissionBridge {
        PermissionBridge::new(std::sync::Arc::new(
            AiStore::open_in_memory().expect("the in-memory store opens"),
        ))
    }

    #[test]
    fn register_respond_round_trips_one_shot() {
        let bridge = bridge();
        bridge
            .register(approval("perm-1", Some("corr-1")))
            .expect("registers");
        assert_eq!(bridge.pending().len(), 1);

        let consumed = bridge
            .respond("perm-1", true, false)
            .expect("the answer consumes the ask");
        assert_eq!(consumed.correlation.as_deref(), Some("corr-1"));
        assert!(bridge.pending().is_empty());

        let again = bridge.respond("perm-1", true, false);
        assert!(again.is_err(), "an answer is one-shot");
    }

    #[test]
    fn duplicate_ids_are_refused() {
        let bridge = bridge();
        bridge
            .register(approval("perm-1", None))
            .expect("registers");
        assert!(bridge.register(approval("perm-1", None)).is_err());
        assert_eq!(bridge.pending().len(), 1);
    }

    /// The audit row a settled save_draft ask wrote, read straight from
    /// the ledger file: the store only appends, so the test brings its
    /// own connection.
    fn recorded_preview(dir: &std::path::Path) -> String {
        let conn = rusqlite::Connection::open(dir.join("ai.sqlite"))
            .expect("the ledger opens for reading");
        conn.query_row(
            "SELECT arg_preview FROM audit_log WHERE verb = 'permission save_draft'",
            [],
            |row| row.get(0),
        )
        .expect("the permission row reads back")
    }

    #[test]
    fn the_audit_row_carries_shapes_not_message_text() {
        let dir = super::super::testkit::scratch("permission-preview");
        let bridge = PermissionBridge::new(std::sync::Arc::new(
            AiStore::open(&dir.join("ai.sqlite")).expect("the file store opens"),
        ));
        bridge
            .register(PendingApproval {
                id: "perm-1".to_string(),
                tool_call_id: "call-1".to_string(),
                tool: "save_draft".to_string(),
                level: PermissionLevel::Confirm,
                args: serde_json::json!({
                    "account": "acct-1",
                    "from": "ada@example.com",
                    "to": ["bob@example.com", "carol@example.com"],
                    "cc": ["dan@example.com"],
                    "subject": "Re: the meridian code is halcyon-dawn",
                    "body": "burn after reading: zulu-seven-bravo",
                }),
                preview: None,
                correlation: Some("corr-1".to_string()),
            })
            .expect("registers");
        bridge
            .respond("perm-1", true, false)
            .expect("the answer consumes the ask");

        let preview = recorded_preview(&dir);
        assert_eq!(
            preview, "account=acct-1 from=1 to=2 cc=1 subject_chars=37 body_chars=36",
            "the preview mirrors the CLI's shape vocabulary"
        );
        for planted in ["example.com", "halcyon-dawn", "zulu-seven-bravo"] {
            assert!(
                !preview.contains(planted),
                "the recorded row leaked '{planted}': {preview}"
            );
        }
    }

    #[test]
    fn fail_close_drains_and_restores_survive() {
        let bridge = bridge();
        bridge
            .register(approval("perm-1", Some("corr-1")))
            .expect("registers");
        let drained = bridge.fail_close_all("sidecar died");
        assert_eq!(drained.len(), 1);
        assert!(bridge.pending().is_empty());

        bridge.restore(drained[0].clone());
        assert_eq!(bridge.pending().len(), 1);
    }
}
