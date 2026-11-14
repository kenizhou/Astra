// SPDX-License-Identifier: GPL-3.0-only
//! The core's [`CategoriesPort`] impl: the category leg of the app bridge, its
//! own child of the port module (a distinct trait from `AppPort`, so its impl
//! splits freely). Each verb hands through to the core's own body and maps the
//! typed refusal to the display sentence the webview surfaces.

use astra_app::{App, RowRef};
use astra_categories::{CategoryDef, CategoryId};
use async_trait::async_trait;

use super::super::{CategoriesPort, HostProvider};

#[async_trait]
impl CategoriesPort for App<HostProvider> {
    async fn list_categories(&self) -> Result<Vec<CategoryDef>, String> {
        App::list_categories(self).map_err(|err| err.to_string())
    }

    async fn create_category(&self, name: &str, color: &str) -> Result<CategoryDef, String> {
        App::create_category(self, name, color).map_err(|err| err.to_string())
    }

    async fn update_category(
        &self,
        id: &CategoryId,
        name: &str,
        color: &str,
    ) -> Result<(), String> {
        App::update_category(self, id, name, color).map_err(|err| err.to_string())
    }

    async fn delete_category(&self, id: &CategoryId) -> Result<u64, String> {
        App::delete_category(self, id)
            .await
            .map_err(|err| err.to_string())
    }

    async fn assign_category(&self, row: RowRef, category: &CategoryId) -> Result<bool, String> {
        App::assign_category(self, row, category)
            .await
            .map_err(|err| err.to_string())
    }

    async fn unassign_category(&self, row: RowRef, category: &CategoryId) -> Result<bool, String> {
        App::unassign_category(self, row, category)
            .await
            .map_err(|err| err.to_string())
    }
}
