// SPDX-License-Identifier: GPL-3.0-only
//! The Unix domain socket listener (macOS and Linux).
//!
//! Three things guard the endpoint, and none of them is a token: the
//! parent directory is 0700 so no other user can traverse into it, the
//! socket itself is owner-only after bind, and every accepted connection's
//! peer uid is checked against the socket file's owner, which is by
//! definition the process that bound it. Reading our own effective uid
//! directly would mean `geteuid`, which is `unsafe`, and this workspace
//! forbids `unsafe` outright; the file's owner is the same fact, read
//! safely through `std`.
//!
//! This module compiles only on a Unix host; the design's known gaps
//! record that the path is unproven until a Unix CI host runs it.

use std::io::{self, ErrorKind};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::net::{UnixListener, UnixStream};

use crate::server::Server;

/// A bound socket: the listener, the file it owns, and the uid every
/// peer must run as.
pub(super) struct SocketListener {
    /// The accepting listener.
    listener: UnixListener,
    /// The socket file, unlinked when the listener drops or its accept
    /// future is aborted, so a client dialling a stopped listener is
    /// refused rather than left hanging.
    file: OwnedSocket,
    /// The uid the socket's owner runs as, which is this process's.
    uid: u32,
}

/// Binds `path`: the directory prepared, any stale leftover claimed, and
/// the socket made owner-only.
pub(super) fn bind(path: &str) -> io::Result<SocketListener> {
    let path = Path::new(path);
    prepare_directory(path)?;
    claim(path)?;
    let listener = UnixListener::bind(path)?;
    // Owner-only whatever the process umask allowed.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    let uid = std::fs::metadata(path)?.uid();
    Ok(SocketListener {
        listener,
        file: OwnedSocket(path.to_owned()),
        uid,
    })
}

impl SocketListener {
    /// Accepts connections forever, serving each and refusing any that
    /// arrives from another uid. A per-connection accept error (a hit
    /// descriptor limit, an interrupted syscall) is transient and the
    /// listener stays good, so the loop keeps serving.
    pub(super) async fn accept(self, server: Arc<Server>) {
        log::info!("mcp: listening for assistant connections");
        loop {
            match self.listener.accept().await {
                Ok((stream, _)) => {
                    if !peer_is(&stream, self.uid) {
                        log::warn!("mcp: refused a connection from another user");
                        continue;
                    }
                    super::spawn_connection(server.clone(), stream);
                }
                Err(err) => log::warn!("mcp: accept failed: {err}"),
            }
        }
    }
}

/// The socket file, unlinked when the listener that owns it goes away.
struct OwnedSocket(PathBuf);

impl Drop for OwnedSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Creates the socket's parent directory and restricts it to this user.
fn prepare_directory(path: &Path) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent)?;
    if let Err(err) = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)) {
        // A directory the system owns refuses the chmod and does not need
        // it (a sandbox container arrives owner-only); what decides is
        // the property the chmod exists to establish.
        if !keeps_others_out(parent) {
            return Err(err);
        }
    }
    Ok(())
}

/// Whether `dir` lets no user but its owner traverse it. A directory
/// carrying no group or other bits cannot be entered by anyone else, and
/// if its owner were another user this process could not create the
/// socket in it either, so there is no case where this answers yes and
/// the endpoint is exposed.
#[allow(clippy::verbose_bit_mask)]
fn keeps_others_out(dir: &Path) -> bool {
    std::fs::metadata(dir).is_ok_and(|meta| meta.permissions().mode() & 0o077 == 0)
}

/// Whether the socket path is ours to bind, never stealing it from a live
/// instance: connect first, and only a leftover that nobody answers may be
/// removed.
fn claim(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    match std::os::unix::net::UnixStream::connect(path) {
        // Somebody is home: this endpoint is theirs.
        Ok(_) => Err(io::Error::new(
            ErrorKind::AddrInUse,
            "another instance owns the endpoint",
        )),
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::ConnectionRefused | ErrorKind::NotFound
            ) =>
        {
            std::fs::remove_file(path)
        }
        // Anything else (a permission error, a path that is not a socket
        // at all) is a state this process must not paper over by deleting
        // a file it does not understand.
        Err(err) => Err(err),
    }
}

/// Whether the connecting process runs as the socket owner's uid, failing
/// closed when the credentials cannot be read.
fn peer_is(stream: &UnixStream, ours: u32) -> bool {
    match stream.peer_cred() {
        Ok(cred) => cred.uid() == ours,
        Err(err) => {
            log::warn!("mcp: could not read peer credentials: {err}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use super::{keeps_others_out, prepare_directory};

    /// A scratch directory at `mode`, removed when the test ends.
    fn dir_at(mode: u32, name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("astra-mcp-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[test]
    fn a_directory_others_can_enter_does_not_satisfy_the_rule() {
        for mode in [0o755, 0o750, 0o705, 0o777] {
            let dir = dir_at(mode, &format!("open{mode:o}"));
            assert!(
                !keeps_others_out(&dir),
                "mode {mode:o} lets another user traverse in"
            );
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn an_owner_only_directory_satisfies_the_rule() {
        let dir = dir_at(0o700, "owner-only");
        assert!(keeps_others_out(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_directory_never_satisfies_the_rule() {
        assert!(!keeps_others_out(Path::new("/astra-mcp-no-such-directory")));
    }

    #[test]
    fn an_existing_owner_only_directory_is_accepted() {
        let dir = dir_at(0o700, "prepare");
        assert!(prepare_directory(&dir.join("mcp.sock")).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }
}
