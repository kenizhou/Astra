// SPDX-License-Identifier: GPL-3.0-only
// The composer document's wire shape, mirrored from astra-composer's serde
// exactly as its tests pin it: enums cross externally tagged with their
// CamelCase variant names, the transparent id newtypes cross as bare strings,
// and every optional field crosses as null rather than absent. ipc.ts re-exports
// the whole set so it stays the one import point of the wire.

/** A stable client-generated attachment id, naming a part of one document. */
export type AttachmentId = string;

/** An RFC 2392 content id for an inline MIME part, without angle brackets. */
export type ContentId = string;

/** A host-provided local blob handle for bytes streamed into MIME later. */
export type DraftBlobHandle = string;

/** A validated `#rrggbb` colour, the only spelling the wire carries. */
export type TextColor = string;

/** A validated link target a text run points at. */
export type LinkUrl = string;

/** A constrained font size; crosses as the bare variant word. */
export type FontSize = 'Small' | 'Normal' | 'Large' | 'Huge';

/** One text run and its marks. Every mark crosses, null or false when unset. */
export interface TextRun {
  text: string;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  font_size: FontSize | null;
  color: TextColor | null;
  highlight: TextColor | null;
  link: LinkUrl | null;
}

/** An inline image inside text flow, resolving to a CID attachment. */
export interface InlineImage {
  attachment_id: AttachmentId;
  alt_text: string;
  width_px: number | null;
}

/** Inline content inside paragraphs, list items, and table cells. */
export type InlineContent = { Text: TextRun } | { Image: InlineImage };

/** A paragraph block. */
export interface Paragraph {
  content: InlineContent[];
}

/** Whether a list is bulleted or numbered; crosses as the bare variant word. */
export type ListKind = 'Bullet' | 'Ordered';

/** One list item: inline content plus an optional nested sub-list. */
export interface ListItem {
  content: InlineContent[];
  child: List | null;
}

/** A bulleted or ordered list whose items may nest further lists. */
export interface List {
  kind: ListKind;
  items: ListItem[];
}

/** A table cell. */
export interface TableCell {
  content: InlineContent[];
}

/** A table row. */
export interface TableRow {
  cells: TableCell[];
}

/** A rectangular table; every row carries the same cell count. */
export interface Table {
  rows: TableRow[];
}

/** The house style a quoted original renders in; the bare variant word. */
export type QuoteStyle = 'Indented' | 'LineAndHeader';

/** One labelled line of a quote's header block. */
export interface QuoteHeader {
  label: string;
  value: string;
}

/** The attribution above a quoted original, carrying both renderings. */
export interface QuoteAttribution {
  line: string;
  headers: QuoteHeader[];
}

/** A quoted original message embedded below a reply or forward. */
export interface Quote {
  style: QuoteStyle;
  attribution: QuoteAttribution;
  body_html: string;
  body_plain: string;
}

/** The sender's signature block, emitted verbatim. */
export interface Signature {
  body_html: string;
  body_plain: string;
}

/** One block of the composer document, externally tagged. */
export type Block =
  | { Paragraph: Paragraph }
  | { List: List }
  | { Table: Table }
  | { Quote: Quote }
  | { Signature: Signature };

/** Whether an attachment is inline CID content or a regular file. */
export type AttachmentDisposition = { Inline: { cid: ContentId } } | 'Attachment';

/** Attachment metadata plus wherever its bytes are. */
export interface DraftAttachment {
  id: AttachmentId;
  blob: DraftBlobHandle | null;
  file_name: string;
  media_type: string;
  size: number | null;
  disposition: AttachmentDisposition;
  data_url: string | null;
}

/** The shared composer document: the durable shape that leaves the editor. */
export interface ComposerDocument {
  blocks: Block[];
  attachments: DraftAttachment[];
}
