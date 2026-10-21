// SPDX-License-Identifier: GPL-3.0-only
//! The worker's read pump: one compact JSON object per line in, dispatched to
//! the call it answers, the stream it belongs to, or the events channel. A
//! malformed line is a protocol violation and kills the connection (fail
//! closed); a clean EOF means the worker exited, which is the same event.

use std::sync::Arc;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use super::{supervision, Shared, WorkerError, WorkerEvent, WorkerMessage};

/// Writes one envelope as a single line. The newline terminator IS the frame.
pub(crate) async fn write_message(
    writer: &mut (dyn AsyncWrite + Unpin + Send),
    message: &impl serde::Serialize,
) -> Result<(), WorkerError> {
    let mut line =
        serde_json::to_string(message).map_err(|error| WorkerError::Protocol(error.to_string()))?;
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|error| WorkerError::Io(error.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|error| WorkerError::Io(error.to_string()))
}

/// Runs the read loop until EOF or a protocol violation, then reports the
/// death to supervision with the generation the pump was started under.
pub(crate) fn spawn_pump(
    shared: Arc<Shared>,
    generation: u64,
    reader: Box<dyn AsyncBufRead + Unpin + Send>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = reader.lines();
        // A clean EOF means the worker exited, a read error or a protocol
        // violation is the same event for supervision: the connection dies.
        while let Ok(Some(line)) = lines.next_line().await {
            if !dispatch(&shared, &line).await {
                break;
            }
        }
        supervision::handle_dead(&shared, generation).await;
    })
}

/// Handles one line; false means the line was not a worker message and the
/// connection is dead.
async fn dispatch(shared: &Arc<Shared>, line: &str) -> bool {
    let message: WorkerMessage = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(_) => return false,
    };
    match message {
        WorkerMessage::ObjectResult { id, value } => {
            let pending = shared.inner.lock().await.pending.remove(&id);
            if let Some(pending) = pending {
                let _ = pending.send(Ok(value));
            }
        }
        WorkerMessage::ObjectError { id, message } => {
            let pending = shared.inner.lock().await.pending.remove(&id);
            if let Some(pending) = pending {
                let _ = pending.send(Err(WorkerError::Worker(message)));
            }
        }
        WorkerMessage::ObjectEvent { stream, event } => {
            // An event for a stream this worker is not subscribed to is
            // dropped, never delivered: the subscription set is the allowlist.
            if shared.inner.lock().await.subscriptions.contains(&stream) {
                let _ = shared
                    .events_tx
                    .send(WorkerEvent::ObjectEvent { stream, event });
            }
        }
        WorkerMessage::PermissionRequest {
            id,
            capability,
            reason,
        } => {
            let _ = shared.events_tx.send(WorkerEvent::PermissionRequest {
                id,
                capability,
                reason,
            });
        }
        WorkerMessage::PluginEvent { event } => {
            let _ = shared.events_tx.send(WorkerEvent::PluginEvent { event });
        }
        WorkerMessage::PluginReady { .. } => {
            // Ready after the handshake is a protocol violation: the handshake
            // consumes exactly one ready, at init.
            return false;
        }
    }
    true
}
