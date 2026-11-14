// SPDX-License-Identifier: GPL-3.0-only
//! The core colour categories: define a coloured label, assign it to
//! conversations, manage the defs. A LOCAL overlay, so nothing here writes the
//! engine: the definitions and assignments live in the device-local
//! [`CategoryStore`] the host installs, and the list reads them back through
//! the post-projection fold in `categories_overlay`. Assigning a flat message
//! row resolves its thread key through one engine READ (never a write), the
//! same `thread_key` convention the projection groups by.

use std::sync::Arc;

use astra_categories::{CategoryDef, CategoryId, CategoryStore};
use engine_api::Provider;
use thiserror::Error;

use crate::{list_rows, reference::RowRef, App};

/// Why a category verb could not be applied. The typed form lets the host
/// report a readable refusal, never a raw constraint string.
#[derive(Debug, Error)]
pub enum CategoryError {
    /// No category store is installed; a write cannot proceed.
    #[error("the category store is not installed")]
    NoStore,
    /// The message key names nothing in the account's store.
    #[error("the message no longer resolves in the store")]
    Unresolved,
    /// The category store failed.
    #[error("the category store failed: {0}")]
    Store(#[from] astra_categories::CategoryError),
}

impl<P: Provider> App<P> {
    /// Installs the category store the core defines and assigns over. Call it
    /// before the first category verb: an app with no store installed overlays
    /// nothing and refuses writes, the same shape the reminder store installs.
    pub fn install_categories_store(&self, store: Arc<CategoryStore>) {
        *self.categories.lock().expect("categories mutex poisoned") = Some(store);
    }

    /// The installed category store, or `None` when the host has not installed
    /// one.
    fn category_store(&self) -> Option<Arc<CategoryStore>> {
        self.categories
            .lock()
            .expect("categories mutex poisoned")
            .clone()
    }

    /// Creates a definition. The wire carries ids only, so the list needs no
    /// rebuild: the names and colours resolve surface-side from the defs.
    ///
    /// # Errors
    ///
    /// [`CategoryError::NoStore`] when no store is installed, else every store
    /// refusal (validation, a name clash).
    pub fn create_category(&self, name: &str, color: &str) -> Result<CategoryDef, CategoryError> {
        let store = self.category_store().ok_or(CategoryError::NoStore)?;
        Ok(store.create_category(name, color)?)
    }

    /// Renames and recolours a definition in place; the assignments key the id
    /// so they survive. No rebuild: the wire carries ids only.
    ///
    /// # Errors
    ///
    /// [`CategoryError::NoStore`] when no store is installed, else every store
    /// refusal (validation, a name clash, an unknown id).
    pub fn update_category(
        &self,
        id: &CategoryId,
        name: &str,
        color: &str,
    ) -> Result<(), CategoryError> {
        let store = self.category_store().ok_or(CategoryError::NoStore)?;
        store.update_category(id, name, color)?;
        Ok(())
    }

    /// Deletes a definition and cascades its assignments, then rebuilds the
    /// list so the rows shed the gone category. Answers the removed count.
    ///
    /// # Errors
    ///
    /// [`CategoryError::NoStore`] when no store is installed, else an unknown
    /// id or a store failure.
    pub async fn delete_category(&self, id: &CategoryId) -> Result<u64, CategoryError> {
        let store = self.category_store().ok_or(CategoryError::NoStore)?;
        let removed = store.delete_category(id)?;
        self.invalidate_list_cache();
        self.bump_generation();
        self.rebuild_snapshot().await;
        Ok(removed)
    }

    /// Assigns one category to the row's conversation, then rebuilds the list
    /// so the row carries it. A thread row keys its id directly; a message row
    /// resolves its thread key through one engine read. Answers whether this
    /// call inserted (idempotent).
    ///
    /// # Errors
    ///
    /// [`CategoryError::NoStore`] when no store is installed,
    /// [`CategoryError::Unresolved`] when a message row no longer resolves,
    /// else an unknown category or a store failure.
    pub async fn assign_category(
        &self,
        row: RowRef,
        category: &CategoryId,
    ) -> Result<bool, CategoryError> {
        let store = self.category_store().ok_or(CategoryError::NoStore)?;
        let (account, thread) = self.resolve_thread_key(&row).await?;
        let inserted = store.assign_category(&account, &thread, category)?;
        self.invalidate_list_cache();
        self.bump_generation();
        self.rebuild_snapshot().await;
        Ok(inserted)
    }

    /// Removes one category from the row's conversation, then rebuilds the
    /// list. Answers whether a row was removed (idempotent).
    ///
    /// # Errors
    ///
    /// [`CategoryError::NoStore`] when no store is installed,
    /// [`CategoryError::Unresolved`] when a message row no longer resolves,
    /// else a store failure.
    pub async fn unassign_category(
        &self,
        row: RowRef,
        category: &CategoryId,
    ) -> Result<bool, CategoryError> {
        let store = self.category_store().ok_or(CategoryError::NoStore)?;
        let (account, thread) = self.resolve_thread_key(&row).await?;
        let removed = store.unassign_category(&account, &thread, category)?;
        self.invalidate_list_cache();
        self.bump_generation();
        self.rebuild_snapshot().await;
        Ok(removed)
    }

    /// Every definition, creation order. An app with no store answers empty.
    ///
    /// # Errors
    ///
    /// A store failure when the read cannot run.
    pub fn list_categories(&self) -> Result<Vec<CategoryDef>, CategoryError> {
        let Some(store) = self.category_store() else {
            return Ok(Vec::new());
        };
        Ok(store.list_categories()?)
    }

    /// The (account, thread key) a row's assignment keys: a thread row by its
    /// id directly, a message row by its source row's `thread_key` (the
    /// resolved thread id, or its own provider key when never threaded), read
    /// from the engine. A READ, so the local-only red line holds.
    async fn resolve_thread_key(
        &self,
        row: &RowRef,
    ) -> Result<(engine_api::AccountId, String), CategoryError> {
        match row {
            RowRef::Thread(thread) => Ok((thread.account.clone(), thread.thread_id.clone())),
            RowRef::Message(message) => {
                let resolved = self
                    .engine
                    .mail_by_keys(&message.account, std::slice::from_ref(&message.key))
                    .await
                    .unwrap_or_default();
                let source = resolved
                    .into_iter()
                    .next()
                    .ok_or(CategoryError::Unresolved)?;
                Ok((
                    message.account.clone(),
                    list_rows::thread_key(&source).to_owned(),
                ))
            }
        }
    }
}

#[cfg(test)]
#[path = "categories_tests.rs"]
mod tests;
