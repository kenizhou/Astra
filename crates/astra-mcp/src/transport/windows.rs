// SPDX-License-Identifier: GPL-3.0-only
//! The Windows named-pipe listener.
//!
//! # The security model, without `unsafe`
//!
//! A hand-rolled DACL is unavailable here (the raw security-attribute
//! builder is `unsafe`, and this workspace forbids `unsafe` outright) and
//! is not needed: a pipe created with null security attributes inherits
//! the creating token's default DACL, which grants the user, SYSTEM, and
//! Administrators, and nobody else. That inherited descriptor is the
//! same-user boundary; `reject_remote_clients` has the operating system
//! refuse the SMB path in from another machine; and `first_pipe_instance`
//! turns a name collision into a loud bind failure instead of a silent
//! hijack, which is the Windows shape of never-steal-the-endpoint. Every
//! part of the model is a safe call; what it adds up to, and what a
//! one-user one-machine harness can and cannot prove of it, is recorded in
//! the transport module's docs and the design's known gaps.

use std::io;
use std::mem;
use std::sync::Arc;

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

use crate::server::Server;

/// A bound pipe: the instance the next client takes.
pub(super) struct PipeListener {
    /// The pipe's full name, for creating replacement instances.
    name: String,
    /// The instance waiting for the next connection.
    incoming: NamedPipeServer,
}

/// Binds `name`: one pipe instance, created owner-safe (the inherited
/// DACL) and unreachable from off the machine.
pub(super) fn bind(name: &str) -> io::Result<PipeListener> {
    let incoming = ServerOptions::new()
        .first_pipe_instance(true)
        .reject_remote_clients(true)
        .create(name)?;
    Ok(PipeListener {
        name: name.to_owned(),
        incoming,
    })
}

impl PipeListener {
    /// Serves connections forever, one pipe instance at a time.
    ///
    /// The named-pipe pattern differs from a socket's: each instance is
    /// handed to one connecting client and then replaced, so the loop
    /// creates the next instance before serving the current one and a
    /// client never finds the name unlistened. A create failure here ends
    /// the loop (logged): a listener that retries a bind it may never win
    /// is a log-spam generator.
    pub(super) async fn accept(mut self, server: Arc<Server>) {
        log::info!("mcp: listening for assistant connections");
        loop {
            if let Err(err) = self.incoming.connect().await {
                log::warn!("mcp: a pipe connection failed: {err}");
                continue;
            }
            let next = match ServerOptions::new()
                .reject_remote_clients(true)
                .create(&self.name)
            {
                Ok(next) => next,
                Err(err) => {
                    log::warn!("mcp: could not create the next pipe instance: {err}");
                    return;
                }
            };
            let connected = mem::replace(&mut self.incoming, next);
            super::spawn_connection(server.clone(), connected);
        }
    }
}
