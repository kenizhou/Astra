// SPDX-License-Identifier: GPL-3.0-only
//! The `astra-plugin://` scheme: the sandboxed frame's only way to its
//! plugin's bundled assets.
//!
//! A request for `astra-plugin://<plugin-id>/<path>` resolves inside that
//! plugin's own directory only: an unknown id, a traversal (`..`, absolute,
//! backslash), or an escape past the root is refused, and a missing file is a
//! plain not-found. HTML is served with a CSP that allows the bundle's own
//! scripts and styles and nothing remote, so the page a plugin ships is the
//! whole of what can run in its frame.

use std::path::Path;

use tauri::http::{Request, Response};

use super::PluginRegistry;

/// Why an asset request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SchemeError {
    /// The host named no discovered plugin.
    UnknownPlugin,
    /// The path escapes: a `..`, an absolute or drive-anchored path, or a
    /// canonicalised answer outside the plugin's own directory.
    Traversal,
    /// The file does not exist under the plugin's root.
    Missing,
}

/// The CSP every plugin page rides: its bundle's own scripts, styles, images,
/// and fonts, and nothing remote or framed.
pub(crate) const FRAME_CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'";

/// The MIME type an extension serves as; the frame's pages are static assets,
/// so the table stays this small on purpose.
fn mime_of(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html",
        Some("css") => "text/css",
        Some("js" | "mjs") => "text/javascript",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Resolves `rel` inside `root`, refusing every escape shape before touching
/// the disk: a canonicalised answer must stay under the canonical root.
pub(crate) fn resolve_plugin_asset(
    root: &Path,
    rel: &str,
) -> Result<(Vec<u8>, &'static str), SchemeError> {
    let candidate = Path::new(rel);
    if candidate.is_absolute() || rel.contains("..") || rel.contains('\\') {
        return Err(SchemeError::Traversal);
    }
    let root = root
        .canonicalize()
        .map_err(|_| SchemeError::UnknownPlugin)?;
    let joined = root.join(candidate);
    let canonical = joined.canonicalize().map_err(|_| SchemeError::Missing)?;
    if !canonical.starts_with(&root) {
        return Err(SchemeError::Traversal);
    }
    if !canonical.is_file() {
        return Err(SchemeError::Missing);
    }
    let bytes = std::fs::read(&canonical).map_err(|_| SchemeError::Missing)?;
    Ok((bytes, mime_of(&canonical)))
}

/// One `astra-plugin://` request's answer. Splits the URI into the plugin id
/// (the host) and the asset path, then resolves inside that plugin's own
/// directory. HTML answers carry the frame's CSP; every refusal is a 4xx with
/// an empty body.
pub(crate) fn answer_asset(registry: &PluginRegistry, uri: &str) -> Response<Vec<u8>> {
    let refusal = |status: u16| {
        Response::builder()
            .status(status)
            .body(Vec::new())
            .expect("the refusal builds")
    };
    let Some(rest) = uri.strip_prefix("astra-plugin://") else {
        return refusal(400);
    };
    let Some((id, rel)) = rest.split_once('/') else {
        return refusal(400);
    };
    let Some(dir) = registry.bundle_dir(id) else {
        return refusal(404);
    };
    match resolve_plugin_asset(&dir, rel) {
        Ok((bytes, mime)) => {
            let mut builder = Response::builder().status(200).header("content-type", mime);
            if mime == "text/html" {
                builder = builder.header("content-security-policy", FRAME_CSP);
            }
            builder.body(bytes).expect("the answer builds")
        }
        Err(SchemeError::Traversal) => refusal(403),
        Err(SchemeError::UnknownPlugin | SchemeError::Missing) => refusal(404),
    }
}

/// The Tauri registration: every request is answered from the managed state's
/// registry, never from a captured path.
// Tauri's scheme handler hands the request by value; the wrapper only reads its URI.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn handler(app: &tauri::AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    use tauri::Manager;
    let uri = request.uri().to_string();
    match app.try_state::<crate::AppState>() {
        Some(state) => answer_asset(&state.plugins, &uri),
        None => Response::builder()
            .status(503)
            .body(Vec::new())
            .expect("the early refusal builds"),
    }
}

#[cfg(test)]
#[path = "scheme_tests.rs"]
mod tests;
