// SPDX-License-Identifier: GPL-3.0-only
//! The method registry: every object-model method the router can route, what
//! it costs, and how stable it is.
//!
//! The registry is the router's manifest-check partner: a manifest declares
//! method names, the parse rejects names this registry does not define (the
//! custom `plugin.<id>.<name>` namespace excepted, which is the open route for
//! operations no fixed method will ever name), and the router refuses a call
//! whose permission the plugin was not granted.
//!
//! `stable` means the method maps to an intent that exists today;
//! `proposed` means the intent has not landed yet and the method is
//! dev-build-only until it does (the plan's placeholder discipline).

use crate::permissions::Permission;

use super::refs::HandleKind;

/// The object-model node a method sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// The application root.
    AstraMail,
    /// The data root: accounts, folders, lookup by id.
    Session,
    /// One configured account.
    Account,
    /// One folder.
    Folder,
    /// One stored message.
    Message,
    /// One attachment of a message.
    Attachment,
    /// One recipient of a composition.
    Recipient,
    /// The folder/list browsing surface of a window.
    Explorer,
    /// The item surface of a window (reading or composing).
    Inspector,
    /// The composer and its editor.
    Composer,
    /// The settings tree.
    Settings,
    /// The calendar surface.
    Calendar,
    /// The contacts surface.
    People,
    /// The send queue.
    Outbox,
    /// The host-mediated AI plane.
    Agent,
    /// The plugin registry itself.
    Plugins,
}

/// Whether a method's backing intent exists today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stability {
    /// Maps to an existing intent; callable in every build.
    Stable,
    /// No backing intent yet; callable in dev builds only, promoted when the
    /// intent lands.
    Proposed,
}

/// One registered method: its dotted name, the node it sits on, the
/// permission it costs, the handle it takes (and of which kind), and its
/// stability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodDef {
    /// The dotted wire name a manifest declares and an `object_call` carries.
    pub name: &'static str,
    /// The node the method sits on.
    pub node: NodeKind,
    /// The permission a plugin must be granted to call it.
    pub permission: Permission,
    /// The handle the call must carry, and the kind it must be. `None` means
    /// the call names no object and must not carry one.
    pub handle: Option<HandleKind>,
    /// Whether the backing intent exists today.
    pub stability: Stability,
}

/// The registry: every method the object model routes in this slice. The
/// stable rows map to intents that exist today; the proposed rows are the
/// plan's placeholders (send gate, transform, decode, the AI plane), promoted
/// as their slices land.
#[rustfmt::skip]
pub const METHODS: &[MethodDef] = &[
    MethodDef { name: "message.markRead", node: NodeKind::Message, permission: Permission::ReadMailbox, handle: Some(HandleKind::Message), stability: Stability::Stable },
    MethodDef { name: "message.setFlagged", node: NodeKind::Message, permission: Permission::ReadMailbox, handle: Some(HandleKind::Message), stability: Stability::Stable },
    MethodDef { name: "message.archive", node: NodeKind::Message, permission: Permission::ReadMailbox, handle: Some(HandleKind::Message), stability: Stability::Stable },
    MethodDef { name: "message.moveToTrash", node: NodeKind::Message, permission: Permission::ReadMailbox, handle: Some(HandleKind::Message), stability: Stability::Stable },
    MethodDef { name: "message.reply", node: NodeKind::Message, permission: Permission::ComposeEdit, handle: Some(HandleKind::Message), stability: Stability::Stable },
    MethodDef { name: "message.replyAll", node: NodeKind::Message, permission: Permission::ComposeEdit, handle: Some(HandleKind::Message), stability: Stability::Stable },
    MethodDef { name: "message.forward", node: NodeKind::Message, permission: Permission::ComposeEdit, handle: Some(HandleKind::Message), stability: Stability::Stable },
    MethodDef { name: "message.send", node: NodeKind::Message, permission: Permission::Send, handle: Some(HandleKind::Message), stability: Stability::Proposed },
    MethodDef { name: "message.decode", node: NodeKind::Message, permission: Permission::ReadItem, handle: Some(HandleKind::Message), stability: Stability::Proposed },
    MethodDef { name: "folder.createSubfolder", node: NodeKind::Folder, permission: Permission::ReadMailbox, handle: Some(HandleKind::Folder), stability: Stability::Stable },
    MethodDef { name: "explorer.selectFolder", node: NodeKind::Explorer, permission: Permission::ReadItem, handle: Some(HandleKind::Folder), stability: Stability::Stable },
    MethodDef { name: "explorer.openMessage", node: NodeKind::Explorer, permission: Permission::ReadItem, handle: Some(HandleKind::Message), stability: Stability::Stable },
    MethodDef { name: "session.getItemFromID", node: NodeKind::Session, permission: Permission::ReadMailbox, handle: Some(HandleKind::Account), stability: Stability::Stable },
    MethodDef { name: "session.getFolderFromID", node: NodeKind::Session, permission: Permission::ReadMailbox, handle: Some(HandleKind::Account), stability: Stability::Stable },
    MethodDef { name: "composer.transform", node: NodeKind::Composer, permission: Permission::ComposeEdit, handle: Some(HandleKind::Draft), stability: Stability::Proposed },
    MethodDef { name: "composer.setOption", node: NodeKind::Composer, permission: Permission::ComposeEdit, handle: Some(HandleKind::Draft), stability: Stability::Proposed },
    MethodDef { name: "before_send", node: NodeKind::AstraMail, permission: Permission::Send, handle: None, stability: Stability::Proposed },
    MethodDef { name: "agent.summarize", node: NodeKind::Agent, permission: Permission::AiInfer, handle: Some(HandleKind::Message), stability: Stability::Proposed },
    MethodDef { name: "agent.classify", node: NodeKind::Agent, permission: Permission::AiInfer, handle: None, stability: Stability::Proposed },
    MethodDef { name: "agent.replySuggest", node: NodeKind::Agent, permission: Permission::AiInfer, handle: Some(HandleKind::Message), stability: Stability::Proposed },
    MethodDef { name: "agent.composeAssist", node: NodeKind::Agent, permission: Permission::AiInfer, handle: Some(HandleKind::Draft), stability: Stability::Proposed },
    MethodDef { name: "agent.agentTask", node: NodeKind::Agent, permission: Permission::AiInfer, handle: None, stability: Stability::Proposed },
];

/// The registry entry for a dotted method name, when the name is registered.
#[must_use]
pub fn lookup(name: &str) -> Option<&'static MethodDef> {
    METHODS.iter().find(|def| def.name == name)
}

/// Whether a method name sits in the custom `plugin.<id>.<name>` namespace:
/// the open route for operations the registry will never name. A custom name
/// still must be declared in the manifest; it just skips the registry lookup.
#[must_use]
pub fn is_custom_method(name: &str) -> bool {
    name.len() > "plugin.".len() && name.starts_with("plugin.")
}

#[cfg(test)]
#[path = "methods_tests.rs"]
mod tests;
