// SPDX-License-Identifier: GPL-3.0-only
//! Builders and fixtures shared by the composer test files: the id and blob
//! newtypes, the attachments every vector starts from, and the full-document
//! HTML wrapper the renderer's output is compared against.

use crate::{
    AttachmentDisposition, AttachmentId, ContentId, DraftAttachment, DraftBlobHandle, FontSize,
    InlineContent, ListItem, TextRun,
};

pub(crate) fn aid(value: &str) -> AttachmentId {
    AttachmentId::new(value).expect("non-empty id")
}

pub(crate) fn blob(value: &str) -> DraftBlobHandle {
    DraftBlobHandle::new(value).expect("non-empty blob")
}

pub(crate) fn cid(value: &str) -> ContentId {
    ContentId::new(value).expect("valid cid")
}

/// Mirrors `render::wrap_document`: the body fragment inside the full HTML document.
pub(crate) fn html_doc(body: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"></head><body>{body}</body></html>"
    )
}

pub(crate) fn text(value: &str) -> InlineContent {
    InlineContent::Text(TextRun {
        text: value.to_owned(),
        bold: false,
        italic: false,
        underline: false,
        font_size: None,
        color: None,
        highlight: None,
        link: None,
    })
}

pub(crate) fn rich_text(value: &str) -> InlineContent {
    InlineContent::Text(TextRun {
        text: value.to_owned(),
        bold: true,
        italic: true,
        underline: true,
        font_size: Some(FontSize::Large),
        color: None,
        highlight: None,
        link: None,
    })
}

pub(crate) fn item(content: Vec<InlineContent>) -> ListItem {
    ListItem {
        content,
        child: None,
    }
}

pub(crate) fn inline_attachment(id: &str) -> DraftAttachment {
    DraftAttachment {
        id: aid(id),
        blob: Some(blob("blob://inline")),
        file_name: "chart.png".to_owned(),
        media_type: "image/png".to_owned(),
        size: Some(42),
        disposition: AttachmentDisposition::Inline {
            cid: cid("chart-1@example.test"),
        },
        data_url: None,
    }
}

/// A one-pixel PNG as the editor would hand it over after a paste.
pub(crate) const PASTED_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// An inline image whose bytes travel in the document rather than behind a host handle: what a
/// pasted or dropped picture looks like by the time it reaches Rust.
pub(crate) fn pasted_attachment(id: &str, data_url: &str) -> DraftAttachment {
    DraftAttachment {
        id: aid(id),
        blob: None,
        file_name: "pasted.png".to_owned(),
        media_type: "image/png".to_owned(),
        size: None,
        disposition: AttachmentDisposition::Inline {
            cid: cid("pasted-1@example.test"),
        },
        data_url: Some(data_url.to_owned()),
    }
}

pub(crate) fn regular_attachment(id: &str) -> DraftAttachment {
    DraftAttachment {
        id: aid(id),
        blob: Some(blob("blob://file")),
        file_name: "report.pdf".to_owned(),
        media_type: "application/pdf".to_owned(),
        size: Some(1024),
        disposition: AttachmentDisposition::Attachment,
        data_url: None,
    }
}
