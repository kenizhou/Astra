// SPDX-License-Identifier: GPL-3.0-only
// The picture width writer's contract: a dragged width lands on the
// retained image block it names (and nothing else in the document), the
// store carries it for the autosave, and the document the submit builds
// holds it as the image inline's width_px.

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { setImageWidth, withImageWidth } from './pictureWidth';
import { useComposerStore } from './composerStore';
import { editorToDocument, type EditorDocument } from './documentWire';
import type { DraftAttachment, InlineContent } from '@/services/ipc';

function png(name = 'photo.png'): File {
  return new File([new Uint8Array([1, 2])], name, { type: 'image/png' });
}

/** The document's retained image inlines, in staging order. */
function imagesOf(editor: EditorDocument): Array<{ attachment_id: string; width_px: number | null }> {
  return editor.retained.flatMap(({ block }) =>
    'Paragraph' in block
      ? block.Paragraph.content.filter((inline): inline is Extract<InlineContent, { Image: unknown }> => 'Image' in inline).map((inline) => inline.Image)
      : [],
  );
}

/** A carried row: pre-attached by a resume, holding no bytes client-side. */
function carriedRow(id: string): DraftAttachment {
  return {
    id,
    blob: null,
    file_name: `${id}.png`,
    media_type: 'image/png',
    size: 512,
    disposition: { Inline: { cid: `cid-${id}` } },
    data_url: null,
  };
}

beforeEach(() => {
  useComposerStore.getState().closePane();
  useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
});

afterEach(() => {
  useComposerStore.getState().closePane();
});

describe('the pure writer', () => {
  it('answers the same value when the document names no such picture', () => {
    useComposerStore.setState({
      editor: {
        content: [],
        retained: [
          { index: 1_000_000, block: { Paragraph: { content: [{ Image: { attachment_id: 'pic-1', alt_text: 'one', width_px: null } }] } } },
        ],
        attachments: [carriedRow('pic-1')],
      },
    });
    const editor = useComposerStore.getState().editor;

    // A reference the attachment does not have leaves the document
    // untouched, answered as the same value: no new object, no re-render.
    expect(withImageWidth(editor, 'pic-2', 260)).toBe(editor);
  });

  it('sets the width on every retained reference naming the attachment', () => {
    useComposerStore.setState({
      editor: {
        content: [],
        retained: [
          { index: 1_000_000, block: { Paragraph: { content: [{ Image: { attachment_id: 'pic-1', alt_text: 'one', width_px: null } }] } } },
          { index: 1_000_001, block: { Paragraph: { content: [{ Image: { attachment_id: 'pic-2', alt_text: 'two', width_px: 100 } }] } } },
        ],
        attachments: [carriedRow('pic-1'), carriedRow('pic-2')],
      },
    });
    const editor = useComposerStore.getState().editor;
    const next = withImageWidth(editor, 'pic-2', 260);

    expect(imagesOf(next)).toEqual([
      { attachment_id: 'pic-1', alt_text: 'one', width_px: null },
      { attachment_id: 'pic-2', alt_text: 'two', width_px: 260 },
    ]);
  });
});

describe('the store write', () => {
  it('lands the dragged width on the retained block', () => {
    useComposerStore.getState().pasteImages([png(), png('shot.png')]);
    const id = useComposerStore.getState().editor.attachments[0]!.id;

    setImageWidth(id, 260);

    expect(imagesOf(useComposerStore.getState().editor)[0]).toEqual({
      attachment_id: id,
      alt_text: 'photo.png',
      width_px: 260,
    });
  });

  it('carries the width into the document the submit sends', () => {
    // The submit reads the store's editor through editorToDocument; the
    // width a drag wrote has to ride that document as the image inline's
    // width_px, with no other field of the inline moving.
    useComposerStore.getState().pasteImages([png()]);
    const id = useComposerStore.getState().editor.attachments[0]!.id;

    setImageWidth(id, 440);

    const document = editorToDocument(useComposerStore.getState().editor);
    const inline = document.blocks.flatMap((block) =>
      'Paragraph' in block
        ? block.Paragraph.content.filter((entry): entry is Extract<InlineContent, { Image: unknown }> => 'Image' in entry)
        : [],
    )[0]!;
    expect(inline.Image).toEqual({ attachment_id: id, alt_text: 'photo.png', width_px: 440 });
  });
});
