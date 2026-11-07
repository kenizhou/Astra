// SPDX-License-Identifier: GPL-3.0-only
//! The showcase's MCP backend: the whole [`MailBackend`] surface at its
//! honest empty. The listener never starts in a showcase run (the enable
//! flag is session state the panel owns, and no panel opens one), so the
//! impl exists only to give the host's server the port it is built over.

use astra_app::{MailWriteError, MessagePage, OpenDraftError, QueryAccount};
use astra_viewmodel::FolderRow;

use astra_mcp::backend::{AgentDraft, MailBackend};

use super::app::ShowcaseApp;

#[async_trait::async_trait]
impl MailBackend for ShowcaseApp {
    async fn accounts(&self) -> Vec<QueryAccount> {
        Vec::new()
    }

    async fn folders(&self, _account: &str) -> Vec<FolderRow> {
        Vec::new()
    }

    async fn folder_page(
        &self,
        _account: &str,
        _folder: Option<&str>,
        _unread_only: bool,
        offset: usize,
        _limit: usize,
    ) -> MessagePage {
        MessagePage {
            rows: Vec::new(),
            total: 0,
            offset,
            windowed: false,
        }
    }

    async fn search(
        &self,
        _query: &str,
        _account: Option<&str>,
        _folder: Option<&str>,
        offset: usize,
        _limit: usize,
    ) -> MessagePage {
        MessagePage {
            rows: Vec::new(),
            total: 0,
            offset,
            windowed: false,
        }
    }

    async fn search_set(
        &self,
        _query: &str,
        _accounts: &[String],
        offset: usize,
        _limit: usize,
    ) -> MessagePage {
        MessagePage {
            rows: Vec::new(),
            total: 0,
            offset,
            windowed: false,
        }
    }

    async fn message(&self, _account: &str, _key: &str) -> Option<astra_app::MessageDetail> {
        None
    }

    async fn mark_read(
        &self,
        _account: &str,
        _key: &str,
        _read: bool,
    ) -> Result<(), MailWriteError> {
        Err(MailWriteError::UnknownAccount)
    }

    async fn set_flagged(
        &self,
        _account: &str,
        _key: &str,
        _flagged: bool,
    ) -> Result<(), MailWriteError> {
        Err(MailWriteError::UnknownAccount)
    }

    async fn archive(&self, _account: &str, _key: &str) -> Result<(), MailWriteError> {
        Err(MailWriteError::UnknownAccount)
    }

    async fn trash(&self, _account: &str, _key: &str) -> Result<(), MailWriteError> {
        Err(MailWriteError::UnknownAccount)
    }

    async fn open_composer(&self, _draft: AgentDraft) -> Result<(), OpenDraftError> {
        Err(OpenDraftError::NoAccount)
    }
}
