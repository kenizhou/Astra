// SPDX-License-Identifier: GPL-3.0-only
//! Shared rich-message composer contract for Astra Mail.
//!
//! The platform editors own selection, IME behaviour, undo, and table interaction.
//! This crate owns the durable shape that leaves the editor: a validated document,
//! deterministic outgoing HTML, deterministic `text/plain`, and a manifest that
//! tells MIME assembly which blobs are inline CID resources and which are regular
//! attachments.

mod color;
mod file_meta;
mod host_request;
mod link;
mod list;
mod mailto;
mod quote;
mod render;
mod render_inline;
mod share;
mod signature;
mod types;
mod validate;

pub use color::TextColor;
pub use file_meta::{safe_file_name, safe_media_type};
pub use host_request::{
    answer_script, link_address, parse_host_request, HostAnswer, HostRequest, HostRequestKind,
    LinkAnswer, LinkRequest,
};
pub use link::{has_link_scheme, LinkUrl, LINK_SCHEMES};
pub use list::{List, ListItem, ListKind};
pub use mailto::{parse_mailto, MailtoPrefill};
pub use quote::{Quote, QuoteAttribution, QuoteHeader, QuoteStyle};
pub use render::render;
pub use share::{
    prefill_from_share, RejectedItem, ShareAttachment, SharePrefill, ShareRejection, ShareRequest,
    SharedItem, MAX_SHARED_ITEMS,
};
pub use signature::Signature;
pub use types::{
    AttachmentDisposition, AttachmentId, Block, ComposerDocument, ComposerOutput, ContentId,
    DraftAttachment, DraftBlobHandle, FontSize, InlineContent, InlineImage, OutputAttachment,
    Paragraph, Table, TableCell, TableRow, TextRun,
};
pub use validate::{ComposerError, ComposerResult};

#[cfg(test)]
mod test_support;
