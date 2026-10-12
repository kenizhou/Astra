// SPDX-License-Identifier: GPL-3.0-only
//! The JSON-RPC 2.0 wire format MCP speaks, and its newline framing.
//!
//! Hand-rolled rather than taken from the official Rust SDK. The protocol
//! this server needs is an envelope and three methods (initialize,
//! tools/list, tools/call); the SDK is a large tree with its own release
//! cadence, and every dependency in this workspace carries a justification
//! comment it would struggle to earn. The risk the trade accepts is spec
//! churn, and this module is small enough to absorb it when it arrives.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Parse error: the payload was not valid JSON.
pub(crate) const PARSE_ERROR: i32 = -32_700;
/// The payload was valid JSON but not a valid Request object.
pub(crate) const INVALID_REQUEST: i32 = -32_600;
/// The method does not exist.
pub(crate) const METHOD_NOT_FOUND: i32 = -32_601;
/// The parameters were missing, malformed, or out of range.
pub(crate) const INVALID_PARAMS: i32 = -32_602;
/// An error inside the server.
pub(crate) const INTERNAL_ERROR: i32 = -32_603;

/// One incoming message. A **notification** is a request with no `id`, and
/// MCP sends one of them (`notifications/initialized`), so the distinction
/// is load-bearing: answering a notification is a protocol violation that
/// some clients treat as fatal.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Request {
    /// The protocol tag. Present on every valid message; checked, not
    /// assumed.
    #[serde(default)]
    pub(crate) jsonrpc: String,
    /// The request id, absent on a notification. A JSON value because the
    /// spec allows a string or a number and a client may use either.
    #[serde(default)]
    pub(crate) id: Option<Value>,
    /// The method name.
    pub(crate) method: String,
    /// The parameters, absent for a parameterless method.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) params: Option<Value>,
}

impl Request {
    /// Whether this is a notification (no `id`), which must never be
    /// answered.
    #[must_use]
    pub(crate) fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// One outgoing response; exactly one of `result` or `error`, never both.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Response {
    /// Always `"2.0"`.
    pub(crate) jsonrpc: &'static str,
    /// The id of the request being answered, echoed verbatim. `null` for an
    /// error raised before an id could be read (a parse failure), as the
    /// spec requires.
    pub(crate) id: Value,
    /// The successful result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<Value>,
    /// The failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<RpcError>,
}

impl Response {
    /// A successful response to `id`.
    #[must_use]
    pub(crate) fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    /// A failed response to `id`.
    #[must_use]
    pub(crate) fn err(id: Value, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }

    /// This response as one newline-terminated frame.
    ///
    /// Serialization cannot fail for this type (every field is a plain
    /// value), but an `.expect()` in a socket loop would take the app down
    /// with it, so a failure degrades to a minimal hand-built error frame
    /// instead.
    #[must_use]
    pub(crate) fn frame(&self) -> String {
        match serde_json::to_string(self) {
            Ok(body) => format!("{body}\n"),
            Err(_) => "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\
                 \"response could not be serialized\"}}\n"
                .to_owned(),
        }
    }
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RpcError {
    /// The error code; one of the constants in this module.
    pub(crate) code: i32,
    /// A short, **non-localised** diagnostic. Never carries mail content,
    /// an address, or a search query: this string is written to the
    /// client's own log file.
    pub(crate) message: String,
}

impl RpcError {
    /// Builds an error with `code` and `message`.
    #[must_use]
    pub(crate) fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// The method is not one this server implements.
    #[must_use]
    pub(crate) fn method_not_found(method: &str) -> Self {
        Self::new(METHOD_NOT_FOUND, format!("unknown method: {method}"))
    }

    /// The parameters did not match what the method's schema documents.
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn invalid_params(detail: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, detail)
    }
}

/// Parses one received line into a [`Request`].
///
/// # Errors
///
/// [`PARSE_ERROR`] for malformed JSON, [`INVALID_REQUEST`] for JSON that is
/// not a request object or that carries the wrong protocol version.
pub(crate) fn parse(line: &str) -> Result<Request, RpcError> {
    let request: Request = serde_json::from_str(line)
        .map_err(|err| RpcError::new(PARSE_ERROR, format!("invalid JSON: {err}")))?;
    if request.jsonrpc != "2.0" {
        return Err(RpcError::new(
            INVALID_REQUEST,
            "jsonrpc must be exactly \"2.0\"",
        ));
    }
    Ok(request)
}

#[cfg(test)]
#[path = "../jsonrpc_tests.rs"]
mod tests;
