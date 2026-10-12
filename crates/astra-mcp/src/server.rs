// SPDX-License-Identifier: GPL-3.0-only
//! The MCP server: one connection's traffic over newline-framed JSON-RPC
//! 2.0, the account gate every call passes, and the ledger discipline every
//! call lands under.
//!
//! # The gate
//!
//! The server holds an allowlist of account ids, settable at runtime and
//! persisted nowhere by the server itself (the settings panel owns that).
//! Empty is the default and empty exposes nothing: `list_accounts` answers
//! an empty list, not even a count of what exists, and an unlisted account
//! is refused on every account-scoped tool with a message that does not
//! distinguish "not exposed" from "no such account", because which other
//! mailboxes exist is itself something the user did not agree to disclose.
//! The list is read fresh on every call, so a change applies to the next
//! call on a live connection.
//!
//! # The ledger
//!
//! Every `tools/call` writes one row before its answer goes out: the call's
//! own correlation id, the tool's name, the outcome, the duration, and the
//! argument character count. Never the arguments. A row that cannot land
//! fails its call closed with an internal error rather than passing
//! unrecorded.

use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::backend::MailBackend;
use crate::ledger::ToolLedger;
use crate::server::jsonrpc::{parse, Request, Response, RpcError};
use crate::tools::bridge::{self, ToolFailure};

mod jsonrpc;

/// The protocol revision this server speaks: the newest revision that
/// opens with an `initialize` handshake.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

/// The account allowlist: which accounts an MCP client may see and act on.
///
/// Empty by default, and empty means nothing is exposed. A `BTreeSet` so
/// the panel's reads are stable across writes.
#[derive(Debug, Default)]
pub struct Gate {
    accounts: RwLock<BTreeSet<String>>,
}

impl Gate {
    /// A gate that allows nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the allowed set. The change applies to the next call on any
    /// live connection.
    pub fn set_allowed(&self, ids: impl IntoIterator<Item = String>) {
        *self.accounts.write().expect("gate lock poisoned") = ids.into_iter().collect();
    }

    /// Whether `account` is one the user exposed.
    #[must_use]
    pub fn allows(&self, account: &str) -> bool {
        self.accounts
            .read()
            .expect("gate lock poisoned")
            .contains(account)
    }

    /// The allowed account ids, in stable order.
    #[must_use]
    pub fn allowed(&self) -> Vec<String> {
        self.accounts
            .read()
            .expect("gate lock poisoned")
            .iter()
            .cloned()
            .collect()
    }
}

/// The MCP server over one running app and one ledger sink.
pub struct Server {
    backend: Arc<dyn MailBackend>,
    ledger: Arc<dyn ToolLedger>,
    gate: Gate,
}

impl core::fmt::Debug for Server {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The backend may hold sensitive handles; show nothing of it.
        f.debug_struct("Server").finish_non_exhaustive()
    }
}

impl Server {
    /// Builds a server over `backend` and `ledger`, allowing nothing until
    /// [`Server::gate`] names accounts.
    #[must_use]
    pub fn new(backend: Arc<dyn MailBackend>, ledger: Arc<dyn ToolLedger>) -> Self {
        Self {
            backend,
            ledger,
            gate: Gate::new(),
        }
    }

    /// The account gate, for the host's settings panel to set and read.
    #[must_use]
    pub fn gate(&self) -> &Gate {
        &self.gate
    }

    /// Serves one connection to completion: reads newline-framed requests,
    /// answers each, and returns when the peer closes or the stream errors.
    ///
    /// Notifications are **never** answered; MCP sends
    /// `notifications/initialized` immediately after the handshake, and a
    /// response to it is a protocol violation some clients treat as fatal.
    pub async fn serve_connection<S>(&self, stream: S)
    where
        S: tokio::io::AsyncRead + AsyncWrite + Unpin,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let mut answered = 0_u64;
        loop {
            // A clean close, or a read error, either way this connection is
            // over.
            let Ok(Some(line)) = lines.next_line().await else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }
            let Some(response) = self.handle_line(&line).await else {
                continue;
            };
            answered += 1;
            if writer.write_all(response.frame().as_bytes()).await.is_err()
                || writer.flush().await.is_err()
            {
                break;
            }
        }
        // Counts only; never a method name, an argument, a query, or an
        // address.
        log::info!("mcp: connection closed after {answered} answered request(s)");
    }

    /// Handles one received line, returning the response to write, or
    /// `None` for a notification.
    async fn handle_line(&self, line: &str) -> Option<Response> {
        let request = match parse(line) {
            Ok(request) => request,
            // A parse failure has no id to echo, so the spec's `null` is
            // used.
            Err(error) => return Some(Response::err(Value::Null, error)),
        };
        if request.is_notification() {
            log::debug!("mcp: notification {}", request.method);
            return None;
        }
        let id = request.id.clone().unwrap_or(Value::Null);
        Some(match self.dispatch(&request).await {
            Ok(result) => Response::ok(id, result),
            Err(error) => Response::err(id, error),
        })
    }

    /// Routes one request to its method.
    async fn dispatch(&self, request: &Request) -> Result<Value, RpcError> {
        match request.method.as_str() {
            "initialize" => initialize(request.params.as_ref()),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({
                "tools": crate::tools::tools()
                    .iter()
                    .map(crate::tools::Tool::to_json)
                    .collect::<Vec<_>>(),
            })),
            "tools/call" => self.tools_call(request.params.as_ref()).await,
            other => Err(RpcError::method_not_found(other)),
        }
    }

    /// `tools/call`: run the named tool, land the ledger row, and shape the
    /// result. The row is written whatever the outcome, and a row that
    /// cannot land fails the call closed: an audit trail with holes is an
    /// audit trail nobody can trust.
    async fn tools_call(&self, params: Option<&Value>) -> Result<Value, RpcError> {
        let name = params
            .and_then(|params| params.get("name"))
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("tools/call requires a tool name"))?;
        let args = params
            .and_then(|params| params.get("arguments"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        let started = std::time::Instant::now();
        let outcome = bridge::call(&*self.backend, &self.gate, name, args.clone()).await;
        let record = crate::ledger::ToolCallRecord {
            correlation: astra_ai::CorrelationId::generate(),
            tool: name.to_owned(),
            outcome: outcome_class(&outcome),
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            arg_chars: crate::ledger::arg_chars(&args),
        };
        if let Err(closed) = self.ledger.record(&record) {
            log::error!("mcp: the tool-call ledger refused a row: {closed}");
            return Err(RpcError::new(
                jsonrpc::INTERNAL_ERROR,
                "the tool-call ledger could not record this call; refusing to answer it \
                 unrecorded"
                    .to_owned(),
            ));
        }
        // The tool's name and whether it worked. Never its arguments.
        log::info!(
            "mcp: tools/call {name} -> {}",
            if outcome.is_ok() { "ok" } else { "error" },
        );
        match outcome {
            Ok(structured) => Ok(success(&structured)),
            // A refusal or a bad argument is a TOOL error, not a protocol
            // error: the model should see it, relay it, and possibly try
            // something else.
            Err(ToolFailure::Refused(message) | ToolFailure::BadArgs(message)) => {
                Ok(tool_error(&message))
            }
            Err(ToolFailure::Unknown(name)) => {
                Err(RpcError::method_not_found(&format!("tool {name}")))
            }
        }
    }
}

/// The ledger's outcome word for one finished call.
fn outcome_class(outcome: &Result<Value, ToolFailure>) -> crate::ledger::Outcome {
    match outcome {
        Ok(_) => crate::ledger::Outcome::Ok,
        Err(ToolFailure::Refused(_)) => crate::ledger::Outcome::Refused,
        Err(ToolFailure::BadArgs(_)) => crate::ledger::Outcome::InvalidParams,
        Err(ToolFailure::Unknown(_)) => crate::ledger::Outcome::UnknownTool,
    }
}

/// The handshake: agree a protocol version, and say what this server
/// offers. An unknown version is counter-offered rather than refused, as
/// the spec requires; the client disconnects if it cannot live with the
/// answer, and that decision is its to make.
fn initialize(params: Option<&Value>) -> Result<Value, RpcError> {
    let requested = params
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params("initialize requires a protocolVersion"))?;
    let negotiated = if requested == PROTOCOL_VERSION {
        requested
    } else {
        log::warn!(
            "mcp: client asked for protocol version {requested}, which this build does not \
             know; offering {PROTOCOL_VERSION}",
        );
        PROTOCOL_VERSION
    };
    Ok(json!({
        "protocolVersion": negotiated,
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {
            "name": "astra-mail",
            "title": "Astra Mail",
            "version": env!("CARGO_PKG_VERSION"),
        },
    }))
}

/// A successful tool result: the structured value, plus its JSON as the
/// text fallback an older client renders. Emitting both is free and makes
/// a result machine-checkable rather than prose a model has to parse back.
fn success(structured: &Value) -> Value {
    let text = serde_json::to_string_pretty(structured).unwrap_or_else(|_| structured.to_string());
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured,
        "isError": false,
    })
}

/// A tool-level failure, in the shape MCP defines for one.
fn tool_error(message: &str) -> Value {
    json!({
        "content": [{"type": "text", "text": message}],
        "isError": true,
    })
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
