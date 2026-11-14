// SPDX-License-Identifier: GPL-3.0-only
//! The category fake: the recorded asks and the preset defs, beside the
//! [`CategoriesPort`] impl that serves them. A distinct trait from `AppPort`,
//! so its impl lives in its own child of the port module (one trait's impl
//! cannot split; distinct traits impl freely), keeping the port file under the
//! line cap.

use astra_app::RowRef;
use astra_categories::{CategoryDef, CategoryId};
use async_trait::async_trait;

use crate::app_state::CategoriesPort;
use crate::commands_fakes::FakeApp;

/// The category fake's state: the preset defs and the recorded asks.
#[derive(Default)]
pub(crate) struct CategoriesFake {
    /// The definitions the listing answers with.
    pub(crate) defs: Vec<CategoryDef>,
    /// How many times the listing was pulled.
    pub(crate) listed: usize,
    /// The (name, colour) pairs the create was asked for.
    pub(crate) created: Vec<(String, String)>,
    /// The (id, name, colour) triples the update was asked for.
    pub(crate) updated: Vec<(CategoryId, String, String)>,
    /// The ids the delete was asked for.
    pub(crate) deleted: Vec<CategoryId>,
    /// The (row, category) pairs the assign was asked for.
    pub(crate) assigned: Vec<(RowRef, CategoryId)>,
    /// The (row, category) pairs the unassign was asked for.
    pub(crate) unassigned: Vec<(RowRef, CategoryId)>,
}

#[async_trait]
impl CategoriesPort for FakeApp {
    async fn list_categories(&self) -> Result<Vec<CategoryDef>, String> {
        let mut fake = self.categories.lock().unwrap();
        fake.listed += 1;
        Ok(fake.defs.clone())
    }

    async fn create_category(&self, name: &str, color: &str) -> Result<CategoryDef, String> {
        self.categories
            .lock()
            .unwrap()
            .created
            .push((name.to_owned(), color.to_owned()));
        Ok(CategoryDef {
            id: CategoryId::new(name.to_owned()),
            name: name.to_owned(),
            color: color.to_owned(),
        })
    }

    async fn update_category(
        &self,
        id: &CategoryId,
        name: &str,
        color: &str,
    ) -> Result<(), String> {
        self.categories.lock().unwrap().updated.push((
            id.clone(),
            name.to_owned(),
            color.to_owned(),
        ));
        Ok(())
    }

    async fn delete_category(&self, id: &CategoryId) -> Result<u64, String> {
        self.categories.lock().unwrap().deleted.push(id.clone());
        Ok(0)
    }

    async fn assign_category(&self, row: RowRef, category: &CategoryId) -> Result<bool, String> {
        self.categories
            .lock()
            .unwrap()
            .assigned
            .push((row, category.clone()));
        Ok(true)
    }

    async fn unassign_category(&self, row: RowRef, category: &CategoryId) -> Result<bool, String> {
        self.categories
            .lock()
            .unwrap()
            .unassigned
            .push((row, category.clone()));
        Ok(true)
    }
}
