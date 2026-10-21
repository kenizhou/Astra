// SPDX-License-Identifier: GPL-3.0-only
//! The router: every `object_call` passes three gates before it runs.
//!
//! In order, and each with its own typed refusal:
//!
//! 1. **Manifest.** The method is declared in the plugin's manifest. An
//!    undeclared call is refused without further inspection, which is why the
//!    closed-Intent problem never arises: this envelope is not the astra-app
//!    Intent enum, and the manifest is the whole of what a plugin may name.
//! 2. **Grant.** The method's registry permission is in the plugin's granted
//!    set. A declared-but-ungranted method is refused; a custom
//!    (`plugin.<id>.<name>`) method carries no registry permission and passes
//!    this gate on declaration alone.
//! 3. **Scope.** The call's handle (present exactly when the method expects
//!    one) resolves in the plugin's own handle table and is of the kind the
//!    method names. A folder handle handed to `message.markRead` is
//!    [`RouteError::OutOfScope`], a raw engine key is
//!    [`RouteError::Ref`], and a stale or foreign handle is the ref error the
//!    table returned.
//!
//! Proposed methods (no backing intent yet) route only when the host marks
//! the build a dev build. The router executes nothing itself: it answers a
//! [`RoutedCall`] the worker host turns into an envelope or an intent.

use std::{collections::HashSet, error::Error, fmt};

use serde_json::Value;

use crate::{
    object_model::{
        is_custom_method, lookup, HandleEntry, HandleKind, HandleTable, MethodDef, ObjectRef,
        RefError, Stability,
    },
    Permission, PluginManifest,
};

/// Why a call was refused. Every variant names the gate that fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// Gate 1: the method is not declared in the plugin's manifest.
    UndeclaredMethod(String),
    /// Gate 2: the method's required permission was never granted.
    PermissionDenied {
        /// The method refused.
        method: String,
        /// The permission the plugin lacks.
        permission: Permission,
    },
    /// Gate 3: the handle failed the table's own checks (unknown, stale, or
    /// foreign).
    Ref(RefError),
    /// Gate 3: the method expects a handle and the call carried none.
    MissingHandle {
        /// The method refused.
        method: String,
    },
    /// Gate 3: the method names no object and the call carried a handle
    /// anyway.
    UnexpectedHandle {
        /// The method refused.
        method: String,
    },
    /// Gate 3: the handle resolved but is not the kind the method names.
    OutOfScope {
        /// The refused handle.
        handle: ObjectRef,
        /// The kind the method expected.
        expected: HandleKind,
    },
    /// The method is proposed (no backing intent yet) and this is not a dev
    /// build.
    ProposedMethod(String),
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RouteError::UndeclaredMethod(method) => {
                write!(f, "method {method} is not declared in the manifest")
            }
            RouteError::PermissionDenied { method, permission } => write!(
                f,
                "method {method} requires permission {}, which is not granted",
                permission.wire_name()
            ),
            RouteError::Ref(error) => write!(f, "{error}"),
            RouteError::MissingHandle { method } => {
                write!(
                    f,
                    "method {method} expects a handle and the call carried none"
                )
            }
            RouteError::UnexpectedHandle { method } => {
                write!(
                    f,
                    "method {method} names no object and must not carry a handle"
                )
            }
            RouteError::OutOfScope { handle, expected } => write!(
                f,
                "handle {} is not the {} kind method calls here expect",
                handle.as_str(),
                expected.wire_name()
            ),
            RouteError::ProposedMethod(method) => {
                write!(
                    f,
                    "method {method} is proposed and routes in dev builds only"
                )
            }
        }
    }
}

impl Error for RouteError {}

/// A call that passed all three gates: what the worker host needs to forward
/// or execute it.
#[derive(Debug)]
pub struct RoutedCall<'a> {
    /// The registry entry, absent for a custom-namespace method.
    pub def: Option<&'static MethodDef>,
    /// The method's dotted name.
    pub method: &'a str,
    /// The resolved handle entry, when the method takes a handle. The entry
    /// carries the host-internal key the plugin never sees.
    pub handle: Option<&'a HandleEntry>,
    /// The call's arguments, validated but never interpreted by the router.
    pub args: &'a Value,
}

/// One plugin's routing context: its parsed manifest, its granted
/// permissions, and whether this build routes proposed methods.
#[derive(Debug)]
pub struct Router {
    manifest: PluginManifest,
    grants: HashSet<Permission>,
    dev: bool,
}

impl Router {
    /// A router for one plugin: its manifest, the permissions the host
    /// granted (a subset of the manifest's declared set), and whether
    /// proposed methods route (dev builds only).
    #[must_use]
    pub fn new(manifest: PluginManifest, grants: &[Permission], dev: bool) -> Self {
        Self {
            manifest,
            grants: grants.iter().copied().collect(),
            dev,
        }
    }

    /// The granted permission set this router was built with; the worker
    /// host's init envelope reports exactly these to the plugin.
    pub(crate) fn grants(&self) -> &HashSet<Permission> {
        &self.grants
    }

    /// Routes one call through the three gates.
    ///
    /// # Errors
    ///
    /// Returns the [`RouteError`] naming the gate that fired: manifest,
    /// grant, or scope, in that order.
    pub fn route<'a>(
        &self,
        table: &'a HandleTable,
        handle: &'a Option<ObjectRef>,
        method: &'a str,
        args: &'a Value,
    ) -> Result<RoutedCall<'a>, RouteError> {
        // Gate 1: the manifest must declare the method, registered or custom.
        if !self
            .manifest
            .methods
            .iter()
            .any(|declared| declared == method)
        {
            return Err(RouteError::UndeclaredMethod(method.to_string()));
        }
        let def = lookup(method);
        if def.is_none() && !is_custom_method(method) {
            // A parsed manifest cannot contain one (parse rejects it), but the
            // router refuses again: the check must hold even if the parse
            // contract is ever widened.
            return Err(RouteError::UndeclaredMethod(method.to_string()));
        }
        // Gate 2: a registry method's permission must be granted; a custom
        // method carries no registry permission and passes on declaration.
        if let Some(def) = def {
            if !self.grants.contains(&def.permission) {
                return Err(RouteError::PermissionDenied {
                    method: method.to_string(),
                    permission: def.permission,
                });
            }
            if def.stability == Stability::Proposed && !self.dev {
                return Err(RouteError::ProposedMethod(method.to_string()));
            }
        }
        // Gate 3: the handle must be present exactly when expected, resolve in
        // this plugin's table, and be of the kind the method names.
        let entry = match (def.and_then(|def| def.handle), handle) {
            (Some(expected), Some(handle)) => {
                let entry = table.resolve(handle).map_err(RouteError::Ref)?;
                if entry.kind != expected {
                    return Err(RouteError::OutOfScope {
                        handle: handle.clone(),
                        expected,
                    });
                }
                Some(entry)
            }
            (Some(_), None) => {
                return Err(RouteError::MissingHandle {
                    method: method.to_string(),
                })
            }
            (None, Some(_)) => {
                return Err(RouteError::UnexpectedHandle {
                    method: method.to_string(),
                })
            }
            (None, None) => None,
        };
        Ok(RoutedCall {
            def,
            method,
            handle: entry,
            args,
        })
    }
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod tests;
