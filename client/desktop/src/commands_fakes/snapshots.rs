// SPDX-License-Identifier: GPL-3.0-only
//! The fake app's preset snapshots: the reading pane's answer to an open
//! and the compose family's prefills, split out of the fixture root so
//! that root stays under the 500-line limit. Everything here is
//! `pub(crate)`, re-exported by the parent under the `commands_fakes::…`
//! paths every command test module already uses.

use astra_viewmodel::{ComposeKind, ComposeRequestSnapshot, ReadingSnapshot};
use engine_api::{AccountId, ProviderKey};

/// The snapshot the fake app publishes for the message an open names: the same
/// shape astra-viewmodel's wire tests pin, carrying one attachment.
pub(crate) fn open_snapshot() -> ReadingSnapshot {
    ReadingSnapshot {
        message: astra_viewmodel::MessageKey {
            account: AccountId::try_from("work").expect("an account id"),
            key: ProviderKey::new("imap:v1:u42").expect("a provider key"),
        },
        subject: "Quarterly report".to_owned(),
        from_name: "Ada Lovelace".to_owned(),
        from_address: "ada@example.test".to_owned(),
        sent_at: "2026-10-09T08:30:00Z".to_owned(),
        html: Some("<p>See the report.</p>".to_owned()),
        plain: Some("See the report.".to_owned()),
        attachments: vec![astra_viewmodel::AttachmentRow {
            id: 1,
            file_name: "report.pdf".to_owned(),
            media_type: "application/pdf".to_owned(),
            size: 2048,
        }],
        remote_image_count: 2,
    }
}

/// The prefill the fake computes for one compose dispatch, the core's rules
/// at their simplest: the message's own account composes, a reply answers the
/// original alone, a reply all keeps the original's `Cc`, and a forward names
/// no one and threads under nothing. Shared with the command tests, which
/// assert the pull against exactly what the fake publishes.
pub(crate) fn compose_prefill(kind: ComposeKind, account: &str) -> ComposeRequestSnapshot {
    ComposeRequestSnapshot {
        kind,
        from: AccountId::try_from(account).expect("an account id"),
        to: if kind == ComposeKind::Forward {
            String::new()
        } else {
            "ada@example.test".to_owned()
        },
        cc: if kind == ComposeKind::ReplyAll {
            "carol@example.test".to_owned()
        } else {
            String::new()
        },
        subject: format!(
            "{} Quarterly report",
            if kind == ComposeKind::Forward {
                "Fwd:"
            } else {
                "Re:"
            }
        ),
        in_reply_to: if kind == ComposeKind::Forward {
            None
        } else {
            Some("orig-1@server.test".to_owned())
        },
        document: astra_composer::ComposerDocument {
            blocks: Vec::new(),
            attachments: Vec::new(),
        },
    }
}
