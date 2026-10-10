// SPDX-License-Identifier: GPL-3.0-only
//! The `--json` envelope: the one output shape every verb prints, pinned to a
//! schema version so the sidecar's parser can dispatch on it. Success carries
//! `data`, failure carries `error`, and never both; the fields serialise in a
//! fixed order (`schema`, `ok`, then the one payload field), which the tests
//! pin byte for byte.

use serde::{Deserialize, Serialize};

/// The envelope's schema version. Bumping it is a contract change: the
/// sidecar, the docs, and this constant move together.
pub(crate) const SCHEMA: u32 = 1;

/// How an invocation ended, in the vocabulary the envelope's `error.code`
/// field and the process exit status share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Code {
    /// The verb ran and its envelope carries data. Exit 0.
    Ok,
    /// The verb ran and failed against the store or the engine. Exit 1.
    Runtime,
    /// The arguments did not parse. Exit 2.
    Usage,
    /// A send-class verb, parsed and then refused: humans decide and send.
    /// Exit 3.
    Refused,
}

impl Code {
    /// The process exit status for this outcome.
    pub(crate) fn exit(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Runtime => 1,
            Self::Usage => 2,
            Self::Refused => 3,
        }
    }

    /// The wire word the envelope's `error.code` field spells.
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Runtime => "runtime",
            Self::Usage => "usage",
            Self::Refused => "refused",
        }
    }
}

/// The `error` half of a failed envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ErrorBody {
    /// Which class of failure, one of `usage`, `runtime`, `refused`.
    pub code: String,
    /// What went wrong, in words a caller can act on.
    pub message: String,
}

/// One invocation's whole answer: `{"schema":1,"ok":true,"data":...}` or
/// `{"schema":1,"ok":false,"error":{"code":...,"message":...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Envelope {
    /// The schema version, always [`SCHEMA`].
    schema: u32,
    /// Whether the verb succeeded.
    ok: bool,
    /// The successful verb's payload, absent on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    /// The failure body, absent on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<ErrorBody>,
}

impl Envelope {
    /// A success envelope over `data`.
    pub(crate) fn success(data: serde_json::Value) -> Self {
        Self {
            schema: SCHEMA,
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    /// A failure envelope over a code and message.
    pub(crate) fn failure(code: Code, message: impl Into<String>) -> Self {
        Self {
            schema: SCHEMA,
            ok: false,
            data: None,
            error: Some(ErrorBody {
                code: code.wire().to_owned(),
                message: message.into(),
            }),
        }
    }

    /// The schema version this envelope carries.
    #[cfg(test)]
    pub(crate) fn schema(&self) -> u32 {
        self.schema
    }

    /// The envelope's `ok` flag.
    #[cfg(test)]
    pub(crate) fn is_ok(&self) -> bool {
        self.ok
    }

    /// The payload, on success.
    #[cfg(test)]
    pub(crate) fn data(&self) -> Option<&serde_json::Value> {
        self.data.as_ref()
    }

    /// Renders the envelope as text: one line when `pretty` is false (the
    /// `--json` and piped forms), indented when true (the interactive form).
    ///
    /// # Errors
    /// Fails only if the payload holds a value serde cannot serialise, which
    /// the constructors cannot produce.
    pub(crate) fn render(&self, pretty: bool) -> Result<String, serde_json::Error> {
        if pretty {
            serde_json::to_string_pretty(self)
        } else {
            serde_json::to_string(self)
        }
    }
}
