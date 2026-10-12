// SPDX-License-Identifier: GPL-3.0-only
//! The MCP host fixture for the command tests, split out of the fixture
//! root so that root stays under the 500-line limit: a host over an
//! empty backend and the in-memory ledger, in a scratch data directory
//! whose leaf names the test, so an enabling test binds an endpoint no
//! sibling test holds. The command tests drive the run/stop seam and
//! the gate, never a tool call; the port's mail behaviour is pinned in
//! astra-mcp's own suites.

use std::sync::Arc;

use async_trait::async_trait;

use astra_app::{MailWriteError, MessageDetail, MessagePage, OpenDraftError, QueryAccount};
use astra_mcp::backend::{AgentDraft, MailBackend};
use astra_mcp::ledger::ToolLedger;
use astra_mcp::server::Server;
use astra_viewmodel::FolderRow;

use crate::mcp::McpHost;

/// A backend that answers nothing: the honest empty answer for a port
/// no command test drives with mail.
struct EmptyBackend;

#[async_trait]
impl MailBackend for EmptyBackend {
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

    async fn message(&self, _account: &str, _key: &str) -> Option<MessageDetail> {
        None
    }

    async fn mark_read(
        &self,
        _account: &str,
        _key: &str,
        _read: bool,
    ) -> Result<(), MailWriteError> {
        Ok(())
    }

    async fn set_flagged(
        &self,
        _account: &str,
        _key: &str,
        _flagged: bool,
    ) -> Result<(), MailWriteError> {
        Ok(())
    }

    async fn archive(&self, _account: &str, _key: &str) -> Result<(), MailWriteError> {
        Ok(())
    }

    async fn trash(&self, _account: &str, _key: &str) -> Result<(), MailWriteError> {
        Ok(())
    }

    async fn open_composer(&self, _draft: AgentDraft) -> Result<(), OpenDraftError> {
        Ok(())
    }
}

/// A host over the empty backend and the in-memory ledger, off and
/// exposing nothing, with its endpoint derived from a scratch directory
/// leaf unique to `name`.
pub(crate) fn host(name: &str) -> Arc<McpHost> {
    let server = Server::new(
        Arc::new(EmptyBackend) as Arc<dyn MailBackend>,
        Arc::new(astra_ai::AiStore::open_in_memory().expect("the ledger opens"))
            as Arc<dyn ToolLedger>,
    );
    let dir = super::scratch(name).join(format!("mcp-{name}"));
    std::fs::create_dir_all(&dir).expect("the scratch directory creates");
    Arc::new(McpHost::new(Arc::new(server), &dir))
}
