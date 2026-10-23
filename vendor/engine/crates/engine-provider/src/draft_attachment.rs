//! Draft attachment shapes: the validated `Content-ID` header and the
//! attachment disposition an outbound MIME message carries. Split from
//! `submit.rs` for the file-length budget (a pure move; the old module
//! re-exports everything here).

use core::fmt;

use serde::{Deserialize, Serialize};

/// A validated `Content-ID` header value without surrounding angle brackets.
///
/// The type keeps inline-image references distinct from arbitrary strings and
/// prevents the characters that would break the header boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ContentIdHeader(Box<str>);

impl ContentIdHeader {
    /// Builds a content id value without surrounding angle brackets.
    ///
    /// # Errors
    ///
    /// Returns [`ContentIdError`] when the value is empty or contains a character
    /// that cannot be safely placed inside a `Content-ID` header.
    pub fn new(value: impl Into<String>) -> Result<Self, ContentIdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ContentIdError::Empty);
        }
        if value
            .bytes()
            .any(|b| matches!(b, b'\r' | b'\n' | b'\0' | b'<' | b'>'))
        {
            return Err(ContentIdError::ForbiddenCharacter);
        }
        Ok(Self(value.into_boxed_str()))
    }

    /// Returns the header value without surrounding angle brackets.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ContentIdHeader {
    type Error = ContentIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ContentIdHeader> for String {
    fn from(value: ContentIdHeader) -> Self {
        value.0.into()
    }
}

impl fmt::Display for ContentIdHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a `Content-ID` header value was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentIdError {
    /// The value was empty.
    Empty,
    /// The value contained CR, LF, NUL, `<`, or `>`.
    ForbiddenCharacter,
}

impl fmt::Display for ContentIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("content id must not be empty"),
            Self::ForbiddenCharacter => {
                f.write_str("content id contains a forbidden character (CR, LF, NUL, '<', or '>')")
            }
        }
    }
}

impl std::error::Error for ContentIdError {}

/// Whether a draft attachment is a regular attachment or an inline related part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftAttachmentDisposition {
    /// The part is referenced from the HTML body with a `cid:` URL.
    Inline {
        /// The `Content-ID` value without surrounding angle brackets.
        content_id: ContentIdHeader,
    },
    /// The part should be presented as a regular downloadable attachment.
    Attachment,
}

/// A draft attachment to include in the outbound MIME message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftAttachment {
    /// The display file name.
    pub file_name: String,
    /// The media type, for example `image/png` or `application/pdf`.
    pub media_type: String,
    /// The attachment bytes.
    pub content: Vec<u8>,
    /// How the part is related to the body.
    pub disposition: DraftAttachmentDisposition,
}

impl DraftAttachment {
    /// Builds an inline attachment referenced by a `cid:` URL in the HTML body.
    #[must_use]
    pub fn inline(
        file_name: impl Into<String>,
        media_type: impl Into<String>,
        content_id: ContentIdHeader,
        content: Vec<u8>,
    ) -> Self {
        Self {
            file_name: file_name.into(),
            media_type: media_type.into(),
            content,
            disposition: DraftAttachmentDisposition::Inline { content_id },
        }
    }

    /// Builds a regular downloadable attachment.
    #[must_use]
    pub fn attachment(
        file_name: impl Into<String>,
        media_type: impl Into<String>,
        content: Vec<u8>,
    ) -> Self {
        Self {
            file_name: file_name.into(),
            media_type: media_type.into(),
            content,
            disposition: DraftAttachmentDisposition::Attachment,
        }
    }

    /// Returns the inline content id, if this is an inline attachment.
    #[must_use]
    pub fn content_id(&self) -> Option<&ContentIdHeader> {
        match &self.disposition {
            DraftAttachmentDisposition::Inline { content_id } => Some(content_id),
            DraftAttachmentDisposition::Attachment => None,
        }
    }

    /// Whether this part is an inline related attachment.
    #[must_use]
    pub fn is_inline(&self) -> bool {
        matches!(self.disposition, DraftAttachmentDisposition::Inline { .. })
    }
}
