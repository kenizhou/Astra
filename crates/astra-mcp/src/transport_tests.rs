// SPDX-License-Identifier: GPL-3.0-only
//! Endpoint tests: the validation that runs on every host (a Windows pipe
//! name is malformed on a Unix host too, so the rule is tested wherever
//! the suite runs) and the derivation from the app's data directory that
//! the desktop host owns.

use std::path::Path;

use super::{Endpoint, EndpointError, MAX_SOCKET_PATH};

/// A pipe name is recognised on every host, so the rule is testable
/// anywhere rather than only on the one machine that can run it.
#[test]
fn a_pipe_name_is_recognized_and_validated_on_every_host() {
    let pipe = Endpoint::parse(r"\\.\pipe\com.kenizhou.astra.mcp").unwrap();
    assert_eq!(
        pipe,
        Endpoint::Pipe(r"\\.\pipe\com.kenizhou.astra.mcp".to_owned())
    );
    assert_eq!(pipe.as_str(), r"\\.\pipe\com.kenizhou.astra.mcp");
    assert_eq!(
        Endpoint::parse(r"\\.\pipe\"),
        Err(EndpointError::NotAPipeName),
        "a bare prefix names no pipe",
    );
}

/// An empty endpoint is an error, never a default.
#[test]
fn an_empty_endpoint_is_an_error_not_a_default() {
    assert_eq!(Endpoint::parse("   "), Err(EndpointError::Empty));
}

/// A socket path that cannot fit `sun_path` fails here, with the length in
/// the message, rather than at bind with a mystery error nobody connects
/// to a path limit.
#[test]
fn a_socket_path_past_the_sun_path_limit_is_rejected_with_the_length() {
    let ok = "/".repeat(MAX_SOCKET_PATH - 1);
    assert!(matches!(Endpoint::parse(&ok), Ok(Endpoint::Unix(_))));
    let too_long = "/".repeat(MAX_SOCKET_PATH);
    assert_eq!(
        Endpoint::parse(&too_long),
        Err(EndpointError::TooLong {
            length: MAX_SOCKET_PATH + 1
        }),
    );
}

/// The endpoint derivation on Windows: the pipe name is the data
/// directory's final component plus `.mcp`, so the standard directory
/// yields the stable name and a dev directory beside it cannot collide.
#[test]
#[cfg(windows)]
fn the_pipe_name_is_derived_from_the_data_directory_leaf() {
    let standard =
        Endpoint::for_data_dir(Path::new(r"C:\Users\me\AppData\Roaming\com.kenizhou.astra"));
    assert_eq!(
        standard,
        Endpoint::Pipe(r"\\.\pipe\com.kenizhou.astra.mcp".to_owned()),
    );
    let dev = Endpoint::for_data_dir(Path::new(
        r"C:\Users\me\AppData\Roaming\com.kenizhou.astra.dev",
    ));
    assert_ne!(standard, dev, "a dev build must not collide with the app's");
}

/// The endpoint derivation on Unix: the socket is a file in the data
/// directory, whose 0700 parent carries the user boundary.
#[test]
#[cfg(unix)]
fn the_socket_path_is_derived_from_the_data_directory() {
    let endpoint = Endpoint::for_data_dir(Path::new("/home/me/.local/share/com.kenizhou.astra"));
    assert_eq!(
        endpoint,
        Endpoint::Unix("/home/me/.local/share/com.kenizhou.astra/mcp.sock".to_owned()),
    );
}
