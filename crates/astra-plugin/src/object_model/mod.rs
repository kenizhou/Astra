// SPDX-License-Identifier: GPL-3.0-only
//! The Astra Object Model (AOM): the versioned contract a worker plugin and
//! the host share. The model is not an object graph copied across the process
//! boundary; it is this crate's types plus the envelope discipline, spoken by
//! both sides. `plugin_init` carries the manifest, the granted permissions,
//! the API level, the AOM version, and the worker's data directory, never the
//! object tree. Reads are `object_call`s answered with immutable snapshots;
//! writes are `object_call`s naming mutating methods the host executes; events
//! travel as `object_subscribe` / `object_event`; large payloads cross by
//! blob reference, never by copy.

mod envelope;
mod methods;
mod refs;
mod snapshots;

pub use envelope::{HostMessage, WorkerMessage};
pub use methods::{is_custom_method, lookup, MethodDef, NodeKind, Stability, METHODS};
pub use refs::{HandleEntry, HandleKind, HandleTable, ObjectRef, RefError};
pub use snapshots::{AccountSnapshot, BlobRef, FolderSnapshot, MessageSnapshot};

/// The plugin API level this crate speaks. A manifest must declare exactly
/// this level; anything else is rejected at parse (see
/// [`crate::PluginManifest`]).
pub const PLUGIN_API_LEVEL: u32 = 1;

/// The AOM version this crate speaks. Pinned on both sides of the boundary;
/// negotiated versioning is a later refinement, not a silent gap.
pub const AOM_VERSION: u32 = 1;
