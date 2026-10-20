// SPDX-License-Identifier: GPL-3.0-only
// The signature seeding rules, the web mirror of the core's own: the block
// seeds beneath the quoted original when the document carries one and at
// the top otherwise, a document already bearing a signature seeds nothing,
// and the From swap replaces only a signature that still equals the seeded
// baseline (an edited signature is the person's and stays).
import { describe, expect, it } from 'vitest';

import { documentToEditor, documentFromPlainText } from './documentWire';
import { seedSignatureBlocks, swapSeededSignature } from './signatureSeeding';
import type { Block, Signature } from '@/services/ipc';

const work: Signature = { body_html: '<p>Ada</p>', body_plain: 'Ada' };
const home: Signature = { body_html: '<p>Addy</p>', body_plain: 'Addy' };

function quote(): Block {
  return {
    Quote: {
      style: 'Indented',
      attribution: { line: '', headers: [] },
      body_html: '<p>original</p>',
      body_plain: 'original',
    },
  };
}

describe('the seeding at open', () => {
  it('seeds at the top of a plain document', () => {
    const blocks = seedSignatureBlocks(documentFromPlainText('Hello').blocks, work);
    expect(blocks.map((block) => Object.keys(block)[0])).toEqual(['Signature', 'Paragraph']);
  });

  it('seeds beneath the quote when the document carries one', () => {
    const blocks = seedSignatureBlocks(
      [quote(), { Paragraph: { content: [] } }],
      work,
    );
    expect(blocks.map((block) => Object.keys(block)[0])).toEqual(['Quote', 'Signature', 'Paragraph']);
  });

  it('seeds nothing when a signature already stands', () => {
    const blocks = seedSignatureBlocks(
      [{ Signature: home }, { Paragraph: { content: [] } }],
      work,
    );
    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toEqual({ Signature: home });
  });
});

describe('the From swap', () => {
  it('swaps a signature that still equals the seeded baseline', () => {
    const editor = documentToEditor({
      blocks: [{ Signature: work }, { Paragraph: { content: [] } }],
      attachments: [],
    });

    const swapped = swapSeededSignature(editor, work, home);
    expect(swapped.retained.map((kept) => kept.block)).toEqual([{ Signature: home }]);
  });

  it('drops the seeded signature when the incoming account seeds nothing', () => {
    const editor = documentToEditor({
      blocks: [{ Signature: work }, { Paragraph: { content: [] } }],
      attachments: [],
    });

    const swapped = swapSeededSignature(editor, work, null);
    expect(swapped.retained).toEqual([]);
  });

  it('leaves an edited signature untouched', () => {
    const editor = documentToEditor({
      blocks: [{ Signature: home }, { Paragraph: { content: [] } }],
      attachments: [],
    });

    const swapped = swapSeededSignature(editor, work, null);
    expect(swapped.retained.map((kept) => kept.block)).toEqual([{ Signature: home }]);
  });

  it('touches nothing when no baseline stands', () => {
    const editor = documentToEditor(documentFromPlainText('Hello'));

    const swapped = swapSeededSignature(editor, null, home);
    expect(swapped).toBe(editor);
  });
});
