// SPDX-License-Identifier: GPL-3.0-only
//! The showcase's [`CategoriesPort`] impl: an honest empty for the listing and
//! a static no-op for every write, the same shape the showcase's other write
//! verbs answer (a screenshot run needs states, not transitions). Its own
//! child of the app module, a distinct trait from `AppPort`.

use astra_app::RowRef;
use astra_categories::{CategoryDef, CategoryId};
use async_trait::async_trait;

use crate::app_state::CategoriesPort;

use super::ShowcaseApp;

#[async_trait]
impl CategoriesPort for ShowcaseApp {
    async fn list_categories(&self) -> Result<Vec<CategoryDef>, String> {
        Ok(Vec::new())
    }

    async fn create_category(&self, name: &str, color: &str) -> Result<CategoryDef, String> {
        Ok(CategoryDef {
            id: CategoryId::new(name.to_owned()),
            name: name.to_owned(),
            color: color.to_owned(),
        })
    }

    async fn update_category(
        &self,
        _id: &CategoryId,
        _name: &str,
        _color: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn delete_category(&self, _id: &CategoryId) -> Result<u64, String> {
        Ok(0)
    }

    async fn assign_category(&self, _row: RowRef, _category: &CategoryId) -> Result<bool, String> {
        Ok(false)
    }

    async fn unassign_category(
        &self,
        _row: RowRef,
        _category: &CategoryId,
    ) -> Result<bool, String> {
        Ok(false)
    }
}
