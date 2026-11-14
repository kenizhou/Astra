// SPDX-License-Identifier: GPL-3.0-only
//! The category leg of the app port: a supertrait of [`super::AppPort`] so each
//! impl's category block lives in its own child file (one trait's impl cannot
//! split across modules, distinct traits impl freely). The verbs mirror the
//! core's own; the host maps the core's typed refusal to a display sentence.

use astra_app::RowRef;
use astra_categories::{CategoryDef, CategoryId};
use async_trait::async_trait;

/// The local colour category verbs the command surface drives. A local
/// overlay: none of these writes the engine.
#[async_trait]
pub(crate) trait CategoriesPort: Send + Sync {
    /// Every definition, creation order; an app with no store answers empty.
    ///
    /// # Errors
    ///
    /// Returns the core's refusal words when the read fails.
    async fn list_categories(&self) -> Result<Vec<CategoryDef>, String>;

    /// Creates a definition, answering the landed row.
    ///
    /// # Errors
    ///
    /// Returns the core's refusal words: no store, a validation refusal, or a
    /// case-insensitive name clash.
    async fn create_category(&self, name: &str, color: &str) -> Result<CategoryDef, String>;

    /// Renames and recolours a definition in place; the assignments survive.
    ///
    /// # Errors
    ///
    /// Returns the core's refusal words: no store, a validation refusal, a
    /// name clash, or an unknown id.
    async fn update_category(&self, id: &CategoryId, name: &str, color: &str)
        -> Result<(), String>;

    /// Deletes a definition and cascades its assignments, answering the
    /// removed count.
    ///
    /// # Errors
    ///
    /// Returns the core's refusal words: no store, or an unknown id.
    async fn delete_category(&self, id: &CategoryId) -> Result<u64, String>;

    /// Assigns one category to the row's conversation, answering whether this
    /// call inserted (idempotent).
    ///
    /// # Errors
    ///
    /// Returns the core's refusal words: no store, an unresolved message row,
    /// or an unknown category.
    async fn assign_category(&self, row: RowRef, category: &CategoryId) -> Result<bool, String>;

    /// Removes one category from the row's conversation, answering whether a
    /// row was removed (idempotent).
    ///
    /// # Errors
    ///
    /// Returns the core's refusal words: no store, or an unresolved message
    /// row.
    async fn unassign_category(&self, row: RowRef, category: &CategoryId) -> Result<bool, String>;
}
