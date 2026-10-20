// SPDX-License-Identifier: GPL-3.0-only
// The document wire: the split into editor content and retained blocks, the
// two way round trip of every run mark through TipTap JSON, and the loud
// reject that keeps an unrepresentable shape from being dropped in silence.
// The byte for byte literal pins of each block variant live in
// documentWire.literals.test.ts, split so each file keeps under the cap.
import { describe, expect, it } from 'vitest';
import { Editor as CoreEditor } from '@tiptap/core';

import type { Block, ComposerDocument, TextRun } from '@/services/ipc';

import {
  DocumentWireError,
  documentFromPlainText,
  documentToEditor,
  editorToDocument,
  type EditorDocument,
} from './documentWire';
import { trimmedExtensions } from './Editor';

/** A plain run literal, the way a hand typed line crosses. */
function plainRun(text: string): string {
  return `{"Text":{"text":"${text}","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}`;
}

/** A run with every optional field null. */
function run(text: string): TextRun {
  return { text, bold: false, italic: false, underline: false, font_size: null, color: null, highlight: null, link: null };
}

function tableBlock(): Block {
  return {
    Table: { rows: [{ cells: [{ content: [{ Text: run('a') }] }, { content: [{ Text: run('b') }] }] }] },
  };
}

function quoteBlock(style: 'Indented' | 'LineAndHeader' = 'Indented'): Block {
  return {
    Quote: {
      style,
      attribution: { line: 'On 30 Jun 2026 at 14:03, Alice wrote:', headers: [] },
      body_html: '<p>Original</p>',
      body_plain: 'Original',
    },
  };
}

function signatureBlock(): Block {
  return { Signature: { body_html: '<p>Ada Lovelace</p>', body_plain: 'Ada Lovelace' } };
}

function inlineAttachment() {
  return {
    id: 'att-1',
    blob: 'blob-7',
    file_name: 'chart.png',
    media_type: 'image/png',
    size: 2048,
    disposition: { Inline: { cid: 'chart@astra' } },
    data_url: null,
  };
}

describe('the wire to the editor', () => {
  it('splits the document into editor content and retained blocks', () => {
    const editable: Block = {
      Paragraph: {
        content: [
          { Text: { ...run('the launch is'), link: 'https://astra.example' } },
          { Text: { ...run(' ready'), bold: true } },
        ],
      },
    };
    const quote = quoteBlock();
    const seeded = documentToEditor({ blocks: [editable, quote], attachments: [] });

    expect(seeded.retained).toEqual([{ index: 1, block: quote }]);
    expect(seeded.attachments).toEqual([]);
    expect(seeded.content).toEqual([
      {
        type: 'paragraph',
        content: [
          { type: 'text', marks: [{ type: 'link', attrs: { href: 'https://astra.example' } }], text: 'the launch is' },
          { type: 'text', marks: [{ type: 'bold' }], text: ' ready' },
        ],
      },
    ]);
  });

  it('retains tables, quotes, signatures and images', () => {
    const withImage: Block = {
      Paragraph: { content: [{ Image: { attachment_id: 'att-1', alt_text: 'chart', width_px: 320 } }] },
    };
    const seeded = documentToEditor({
      blocks: [tableBlock(), quoteBlock(), signatureBlock(), withImage],
      attachments: [],
    });

    expect(seeded.content).toEqual([]);
    expect(seeded.retained.map((entry) => entry.index)).toEqual([0, 1, 2, 3]);
  });

  it('seeds the three widened marks as editable content, not retained blocks', () => {
    const marked: Block = {
      Paragraph: {
        content: [
          { Text: { ...run('big'), font_size: 'Large' } },
          { Text: { ...run('tinted'), color: '#112233' } },
          { Text: { ...run('marked'), highlight: '#ffd700' } },
        ],
      },
    };
    const seeded = documentToEditor({ blocks: [marked], attachments: [] });

    expect(seeded.retained).toEqual([]);
    expect(seeded.content).toEqual([
      {
        type: 'paragraph',
        content: [
          { type: 'text', marks: [{ type: 'fontSize', attrs: { size: 'Large' } }], text: 'big' },
          { type: 'text', marks: [{ type: 'color', attrs: { color: '#112233' } }], text: 'tinted' },
          { type: 'text', marks: [{ type: 'highlight', attrs: { color: '#ffd700' } }], text: 'marked' },
        ],
      },
    ]);
  });
});

describe('the editor back to the wire', () => {
  const emptyRest: Pick<EditorDocument, 'retained' | 'attachments'> = { retained: [], attachments: [] };

  it('maps runs, marks, links and both list kinds', () => {
    const content = [
      {
        type: 'paragraph',
        content: [
          { type: 'text', text: 'plain then ' },
          { type: 'text', marks: [{ type: 'bold' }], text: 'bold' },
          { type: 'text', marks: [{ type: 'bold' }], text: ' still' },
          {
            type: 'text',
            marks: [
              { type: 'italic' },
              { type: 'underline' },
              { type: 'link', attrs: { href: 'https://astra.example' } },
            ],
            text: ' linked',
          },
        ],
      },
      {
        type: 'bulletList',
        content: [
          {
            type: 'listItem',
            content: [
              { type: 'paragraph', content: [{ type: 'text', text: 'one' }] },
              {
                type: 'orderedList',
                content: [
                  { type: 'listItem', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'nested' }] }] },
                ],
              },
            ],
          },
        ],
      },
    ];

    expect(JSON.stringify(editorToDocument({ ...emptyRest, content }))).toBe(
      `{"blocks":[{"Paragraph":{"content":[${plainRun('plain then ')},` +
        `{"Text":{"text":"bold still","bold":true,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}},` +
        `{"Text":{"text":" linked","bold":false,"italic":true,"underline":true,"font_size":null,"color":null,"highlight":null,"link":"https://astra.example"}}]}},` +
        `{"List":{"kind":"Bullet","items":[{"content":[${plainRun('one')}],"child":{"kind":"Ordered","items":[{"content":[${plainRun('nested')}],"child":null}]}}]}}],"attachments":[]}`,
    );
  });

  it('splits a paragraph at a hard break and folds one inside a list item', () => {
    const content = [
      {
        type: 'paragraph',
        content: [
          { type: 'text', text: 'above' },
          { type: 'hardBreak' },
          { type: 'text', text: 'below' },
        ],
      },
      {
        type: 'orderedList',
        content: [
          {
            type: 'listItem',
            content: [
              {
                type: 'paragraph',
                content: [
                  { type: 'text', text: 'first' },
                  { type: 'hardBreak' },
                  { type: 'text', text: 'second' },
                ],
              },
            ],
          },
        ],
      },
    ];

    const doc = editorToDocument({ ...emptyRest, content });

    expect(doc.blocks[0]).toEqual({ Paragraph: { content: [{ Text: run('above') }] } });
    expect(doc.blocks[1]).toEqual({ Paragraph: { content: [{ Text: run('below') }] } });
    expect(doc.blocks[2]).toEqual({
      List: { kind: 'Ordered', items: [{ content: [{ Text: run('first\nsecond') }], child: null }] },
    });
  });

  it('splices retained blocks back at their original positions', () => {
    const quote = quoteBlock();
    const signature = signatureBlock();
    const content = [
      { type: 'paragraph', content: [{ type: 'text', text: 'a' }] },
      { type: 'paragraph', content: [{ type: 'text', text: 'added' }] },
    ];

    const doc = editorToDocument({
      content,
      retained: [
        { index: 0, block: quote },
        { index: 2, block: signature },
      ],
      attachments: [{ ...inlineAttachment(), disposition: 'Attachment' }],
    });

    // The retained blocks stand at their original indices; the editor's
    // blocks fill the gaps in order and the surplus follows the last one.
    expect(JSON.stringify(doc.blocks)).toBe(
      `[${JSON.stringify(quote)},{"Paragraph":{"content":[${plainRun('a')}]}},${JSON.stringify(signature)},{"Paragraph":{"content":[${plainRun('added')}]}}]`,
    );
    expect(doc.attachments[0]?.disposition).toBe('Attachment');
  });

  it('emits one empty paragraph for an empty body', () => {
    expect(JSON.stringify(editorToDocument({ ...emptyRest, content: [{ type: 'paragraph' }] }))).toBe(
      '{"blocks":[{"Paragraph":{"content":[]}}],"attachments":[]}',
    );
  });

  it('maps the three widened marks back into their runs, merging only equal ones', () => {
    const content = [
      {
        type: 'paragraph',
        content: [
          { type: 'text', marks: [{ type: 'fontSize', attrs: { size: 'Huge' } }], text: 'headline' },
          { type: 'text', marks: [{ type: 'fontSize', attrs: { size: 'Huge' } }], text: ' more' },
          {
            type: 'text',
            marks: [
              { type: 'color', attrs: { color: '#ff0000' } },
              { type: 'highlight', attrs: { color: '#ffff00' } },
            ],
            text: ' alarm',
          },
        ],
      },
    ];

    expect(JSON.stringify(editorToDocument({ ...emptyRest, content }))).toBe(
      '{"blocks":[{"Paragraph":{"content":[' +
        `{"Text":{"text":"headline more","bold":false,"italic":false,"underline":false,"font_size":"Huge","color":null,"highlight":null,"link":null}},` +
        `{"Text":{"text":" alarm","bold":false,"italic":false,"underline":false,"font_size":null,"color":"#ff0000","highlight":"#ffff00","link":null}}` +
        ']}}],"attachments":[]}',
    );
  });

  it('rejects a font size word outside the four the wire carries loudly', () => {
    expect(() =>
      editorToDocument({
        ...emptyRest,
        content: [
          {
            type: 'paragraph',
            content: [{ type: 'text', marks: [{ type: 'fontSize', attrs: { size: 'Giant' } }], text: 'no' }],
          },
        ],
      }),
    ).toThrow(DocumentWireError);
  });

  it('rejects an unknown node, an unknown mark and a double nested list loudly', () => {
    const itemWithTwoLists = {
      type: 'bulletList',
      content: [
        {
          type: 'listItem',
          content: [
            { type: 'paragraph', content: [{ type: 'text', text: 'one' }] },
            { type: 'bulletList', content: [{ type: 'listItem', content: [] }] },
            { type: 'orderedList', content: [{ type: 'listItem', content: [] }] },
          ],
        },
      ],
    };

    expect(() =>
      editorToDocument({
        ...emptyRest,
        content: [{ type: 'heading', attrs: { level: 1 }, content: [{ type: 'text', text: 'no' }] }],
      }),
    ).toThrow(DocumentWireError);

    expect(() =>
      editorToDocument({
        ...emptyRest,
        content: [{ type: 'paragraph', content: [{ type: 'text', marks: [{ type: 'strike' }], text: 'no' }] }],
      }),
    ).toThrow(DocumentWireError);

    expect(() => editorToDocument({ ...emptyRest, content: [itemWithTwoLists] })).toThrow(
      DocumentWireError,
    );
  });
});

describe('the plain text seed', () => {
  it('builds one paragraph per line', () => {
    expect(JSON.stringify(documentFromPlainText('Half written\nStill here'))).toBe(
      `{"blocks":[{"Paragraph":{"content":[${plainRun('Half written')}]}},{"Paragraph":{"content":[${plainRun('Still here')}]}}],"attachments":[]}`,
    );
  });

  it('seeds an empty body with one empty paragraph', () => {
    expect(documentFromPlainText('')).toEqual({
      blocks: [{ Paragraph: { content: [] } }],
      attachments: [],
    });
  });
});

describe('the round trip through the trimmed schema', () => {
  it('crosses a full editable document through a real editor unchanged', () => {
    const doc: ComposerDocument = {
      blocks: [
        {
          Paragraph: {
            content: [
              { Text: run('the launch is ') },
              { Text: { ...run('ready'), bold: true, underline: true } },
              { Text: { ...run(' with a link'), link: 'https://astra.example' } },
            ],
          },
        },
        {
          List: {
            kind: 'Bullet',
            items: [
              {
                content: [{ Text: run('one') }],
                child: { kind: 'Ordered', items: [{ content: [{ Text: run('nested') }], child: null }] },
              },
              { content: [{ Text: run('two') }], child: null },
            ],
          },
        },
        { Paragraph: { content: [] } },
      ],
      attachments: [],
    };

    const seeded = documentToEditor(doc);
    const editor = new CoreEditor({
      element: document.createElement('div'),
      extensions: trimmedExtensions(),
      content: { type: 'doc', content: seeded.content },
    });

    // The schema accepts every node (it echoes one node per seeded node, of
    // the same kind; it adds only its own default attributes) and the way
    // back rebuilds the document byte for byte.
    const echoed = editor.getJSON().content ?? [];
    expect(echoed.map((node) => node.type)).toEqual(seeded.content.map((node) => node.type));
    expect(JSON.stringify(editorToDocument({ ...seeded, content: echoed }))).toBe(JSON.stringify(doc));
    editor.destroy();
  });

  it('round trips a run carrying every widened mark unchanged', () => {
    const doc: ComposerDocument = {
      blocks: [
        {
          Paragraph: {
            content: [
              { Text: { ...run('the '), font_size: 'Small' } },
              { Text: { ...run('whole'), font_size: 'Large', color: '#123456', highlight: '#654321', bold: true } },
              { Text: { ...run(' idea'), link: 'https://astra.example', color: '#123456' } },
            ],
          },
        },
      ],
      attachments: [],
    };

    const seeded = documentToEditor(doc);

    expect(seeded.retained).toEqual([]);
    expect(JSON.stringify(editorToDocument(seeded))).toBe(JSON.stringify(doc));
  });

  it('round trips a document whose unproducible blocks are retained untouched', () => {
    const doc: ComposerDocument = {
      blocks: [
        { Paragraph: { content: [{ Text: run('hello') }] } },
        tableBlock(),
        quoteBlock(),
        signatureBlock(),
        { Paragraph: { content: [{ Text: run('bye') }] } },
      ],
      attachments: [inlineAttachment()],
    };

    const seeded = documentToEditor(doc);
    expect(seeded.content).toHaveLength(2);

    expect(JSON.stringify(editorToDocument(seeded))).toBe(JSON.stringify(doc));
  });
});
