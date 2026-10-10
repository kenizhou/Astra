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

/// The cap on the argument preview the audit row keeps: enough to recognise
/// the call, never enough to archive the mail it touched.
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
        let mut arg_preview = approval.args.to_string();
        if arg_preview.len() > PREVIEW_CAP {
            arg_preview.truncate(PREVIEW_CAP);
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
