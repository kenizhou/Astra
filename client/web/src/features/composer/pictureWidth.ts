// SPDX-License-Identifier: GPL-3.0-only
// The picture width writer: the one place a dragged width crosses into
// the composition's document, as its own module because the store's own
// file sits at its line cap. The writer answers a new editor value with
// only the named image's width_px moved, so the autosave and the submit
// carry the dragged width with no wire change.

import type { EditorDocument } from './documentWire';
import { useComposerStore } from './composerStore';

/** The document with the named image's width set, every other block,
 *  inline, and field shared with the source. A reference the attachment
 *  does not have leaves the document untouched, answered as the same
 *  value. */
export function withImageWidth(
  editor: EditorDocument,
  attachmentId: string,
  widthPx: number,
): EditorDocument {
  let moved = false;
  const retained = editor.retained.map((kept) => {
    if (!('Paragraph' in kept.block)) {
      return kept;
    }
    let blockMoved = false;
    const content = kept.block.Paragraph.content.map((inline) => {
      if (!('Image' in inline) || inline.Image.attachment_id !== attachmentId) {
        return inline;
      }
      blockMoved = true;
      moved = true;
      return { Image: { ...inline.Image, width_px: widthPx } };
    });
    return blockMoved ? { ...kept, block: { Paragraph: { content } } } : kept;
  });
  return moved ? { ...editor, retained } : editor;
}

/** Writes a dragged width onto the composition's retained image block,
 *  through the store the autosave and the submit read. */
export function setImageWidth(attachmentId: string, widthPx: number): void {
  const editor = useComposerStore.getState().editor;
  const next = withImageWidth(editor, attachmentId, widthPx);
  if (next !== editor) {
    useComposerStore.setState({ editor: next });
  }
}
