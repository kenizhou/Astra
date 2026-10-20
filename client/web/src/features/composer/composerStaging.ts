// SPDX-License-Identifier: GPL-3.0-only
// The composer's attachment arithmetic, split from the store for the line
// cap: one picked File as a document row with its bytes staged behind a
// minted handle, the inline-or-file answer a dropped picture gets, and the
// reference cleanup a removed inline row owes the document. Pure helpers;
// the store holds the state.

import type { Block, DraftAttachment, InlineContent } from '@/services/ipc';

import type { EditorDocument } from './documentWire';

/** One attachment row plus the staged file its bytes live in, when it has
 *  one (a forward's pre-attached originals hold none: the core resolves
 *  their parts at send). */
export interface StagedAttachment {
  row: DraftAttachment;
  file?: File;
}

/** Mints one attachment id: a UUID, unique across the composition. */
function mintId(): string {
  return crypto.randomUUID();
}

/** One picked file as a document row: the bytes stay client-side behind the
 *  minted handle, and the send reads them back out. */
export function attachFile(file: File, inline: boolean): StagedAttachment {
  const id = mintId();
  return {
    row: {
      id,
      blob: mintId(),
      file_name: file.name,
      media_type: file.type === '' ? 'application/octet-stream' : file.type,
      size: file.size,
      disposition: inline ? { Inline: { cid: mintId() } } : 'Attachment',
      data_url: null,
    },
    file,
  };
}

/** The inline reference a picture's inline answer adds to the document, so
 *  the part rides the send as body content: one retained paragraph holding
 *  the image, appended at the document's tail where the trimmed editor
 *  cannot swallow it. */
export function inlineRef(
  editor: EditorDocument,
  attachmentId: string,
  alt: string,
): EditorDocument {
  const block: Block = {
    Paragraph: {
      content: [{ Image: { attachment_id: attachmentId, alt_text: alt, width_px: null } }],
    },
  };
  // The tail index keeps every editable block ahead of the image: the
  // splice-back ordering reads it as "last", whatever the editor holds.
  return {
    ...editor,
    retained: [...editor.retained, { index: 1_000_000, block }],
  };
}

/** The inline content without the images naming `attachmentId`. */
function withoutRefs(content: InlineContent[], attachmentId: string): InlineContent[] {
  return content.filter(
    (inline) => !('Image' in inline && inline.Image.attachment_id === attachmentId),
  );
}

/** Drops every inline reference naming `attachmentId` from the retained
 *  blocks: a removed inline row owes the document its reference, or the
 *  send refuses an inline attachment nothing points at. */
export function stripInlineRefs(
  editor: EditorDocument,
  attachmentId: string,
): EditorDocument {
  const strip = (block: Block): Block =>
    'Paragraph' in block
      ? { Paragraph: { content: withoutRefs(block.Paragraph.content, attachmentId) } }
      : block;
  return {
    ...editor,
    retained: editor.retained.map((kept) => ({ ...kept, block: strip(kept.block) })),
  };
}
