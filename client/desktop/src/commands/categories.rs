// SPDX-License-Identifier: GPL-3.0-only
//! The category command surface: the manager's CRUD and the row menu's
//! Categorize verbs over the core's own category bodies. A child of [`super`],
//! whose command registration this module continues, split the same way the
//! reminder module was to keep each file under the 500-line limit. A local
//! overlay: none of these writes the engine. A malformed row reference is
//! refused before anything reaches the core.

use astra_app::RowRef;
use astra_categories::{CategoryDef, CategoryId};
use tauri::State;

use crate::AppState;

/// Parses the row reference the webview sent, refusing a malformed one before
/// anything reaches the core (the row key stands in for its RowRef twin).
fn parse_row(row: serde_json::Value) -> Result<RowRef, String> {
    serde_json::from_value(row).map_err(|_| "the row reference is malformed".to_owned())
}

/// Every definition, creation order; an app with no store answers empty.
///
/// # Errors
///
/// Returns the core's refusal sentence when the read fails.
pub(crate) async fn categories_list_of(state: &AppState) -> Result<Vec<CategoryDef>, String> {
    state.app.list_categories().await
}

/// Creates a definition, answering the landed row.
///
/// # Errors
///
/// Returns the core's refusal sentence (no store, validation, a name clash).
pub(crate) async fn categories_create_of(
    state: &AppState,
    name: &str,
    color: &str,
) -> Result<CategoryDef, String> {
    state.app.create_category(name, color).await
}

/// Renames and recolours a definition in place; the assignments survive.
///
/// # Errors
///
/// Returns the core's refusal sentence (no store, validation, a clash, an
/// unknown id).
pub(crate) async fn categories_update_of(
    state: &AppState,
    id: &str,
    name: &str,
    color: &str,
) -> Result<(), String> {
    let id = CategoryId::new(id.to_owned());
    state.app.update_category(&id, name, color).await
}

/// Deletes a definition and cascades its assignments, answering the removed
/// count.
///
/// # Errors
///
/// Returns the core's refusal sentence (no store, an unknown id).
pub(crate) async fn categories_delete_of(state: &AppState, id: &str) -> Result<u64, String> {
    let id = CategoryId::new(id.to_owned());
    state.app.delete_category(&id).await
}

/// Assigns one category to the row's conversation, answering whether this call
/// inserted. A malformed row reference is refused before the core.
///
/// # Errors
///
/// Returns the malformed-reference refusal or the core's refusal sentence.
pub(crate) async fn categories_assign_of(
    state: &AppState,
    row: serde_json::Value,
    category_id: &str,
) -> Result<bool, String> {
    let row = parse_row(row)?;
    let category = CategoryId::new(category_id.to_owned());
    state.app.assign_category(row, &category).await
}

/// Removes one category from the row's conversation, answering whether a row
/// was removed. A malformed row reference is refused before the core.
///
/// # Errors
///
/// Returns the malformed-reference refusal or the core's refusal sentence.
pub(crate) async fn categories_unassign_of(
    state: &AppState,
    row: serde_json::Value,
    category_id: &str,
) -> Result<bool, String> {
    let row = parse_row(row)?;
    let category = CategoryId::new(category_id.to_owned());
    state.app.unassign_category(row, &category).await
}

/// Pulls the category definitions the manager and the row badges resolve.
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn categories_list(state: State<'_, AppState>) -> Result<Vec<CategoryDef>, String> {
    state.runtime.block_on(|| categories_list_of(&state))
}

/// Creates a category definition from the manager's name and palette colour.
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn categories_create(
    state: State<'_, AppState>,
    name: String,
    color: String,
) -> Result<CategoryDef, String> {
    state
        .runtime
        .block_on(|| categories_create_of(&state, &name, &color))
}

/// Renames and recolours a category definition in place.
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn categories_update(
    state: State<'_, AppState>,
    id: String,
    name: String,
    color: String,
) -> Result<(), String> {
    state
        .runtime
        .block_on(|| categories_update_of(&state, &id, &name, &color))
}

/// Deletes a category definition and its assignments, answering the removed
/// count the manager reports.
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn categories_delete(state: State<'_, AppState>, id: String) -> Result<u64, String> {
    state.runtime.block_on(|| categories_delete_of(&state, &id))
}

/// Assigns a category to the row's conversation: the Categorize submenu's
/// toggle-on leg.
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn categories_assign(
    state: State<'_, AppState>,
    row: serde_json::Value,
    category_id: String,
) -> Result<bool, String> {
    state
        .runtime
        .block_on(|| categories_assign_of(&state, row, &category_id))
}

/// Removes a category from the row's conversation: the Categorize submenu's
/// toggle-off leg.
///
/// # Errors
///
/// Returns the refusal sentence the webview surfaces.
// Tauri's command macro hands `State` by value; the wrapper only reads through it.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub(crate) fn categories_unassign(
    state: State<'_, AppState>,
    row: serde_json::Value,
    category_id: String,
) -> Result<bool, String> {
    state
        .runtime
        .block_on(|| categories_unassign_of(&state, row, &category_id))
}

#[cfg(test)]
#[path = "categories_tests.rs"]
mod tests;
