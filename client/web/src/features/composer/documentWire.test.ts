// SPDX-License-Identifier: GPL-3.0-only
// The document wire: the literals each block variant crosses with, byte for
// byte against astra-composer's serde (field order is the Rust declaration
// order, optionals cross as null, variants are externally tagged CamelCase),
// the two way round trip through TipTap JSON, and the loud reject that keeps
// an unrepresentable shape from being dropped in silence.
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

describe('the block literals on the wire', () => {
  it('carries a paragraph with one marked sized run', () => {
    const block: Block = {
      Paragraph: { content: [{ Text: { ...run('Half written'), bold: true, font_size: 'Large' } }] },
    };
    expect(JSON.stringify(block)).toBe(
      '{"Paragraph":{"content":[{"Text":{"text":"Half written","bold":true,"italic":false,"underline":false,"font_size":"Large","color":null,"highlight":null,"link":null}}]}}',
    );
  });

  it('spells every font size word the union allows', () => {
    const sizes = ['Small', 'Normal', 'Large', 'Huge'] as const;
    for (const size of sizes) {
      const block: Block = { Paragraph: { content: [{ Text: { ...run('sized'), font_size: size } }] } };
      expect(JSON.stringify(block)).toBe(
        `{"Paragraph":{"content":[{"Text":{"text":"sized","bold":false,"italic":false,"underline":false,"font_size":"${size}","color":null,"highlight":null,"link":null}}]}}`,
      );
    }
  });

  it('carries a bullet list with the child spelled null', () => {
    const block: Block = { List: { kind: 'Bullet', items: [{ content: [{ Text: run('one') }], child: null }] } };
    expect(JSON.stringify(block)).toBe(
      `{"List":{"kind":"Bullet","items":[{"content":[${plainRun('one')}],"child":null}]}}`,
    );
  });

  it('carries an ordered list with a nested bullet child', () => {
    const block: Block = {
      List: {
        kind: 'Ordered',
        items: [
          {
            content: [{ Text: run('first') }],
            child: { kind: 'Bullet', items: [{ content: [{ Text: run('nested') }], child: null }] },
          },
        ],
      },
    };
    expect(JSON.stringify(block)).toBe(
      `{"List":{"kind":"Ordered","items":[{"content":[${plainRun('first')}],"child":{"kind":"Bullet","items":[{"content":[${plainRun('nested')}],"child":null}]}}]}}`,
    );
  });

  it('carries a table of rows and cells with inline content', () => {
    const block: Block = {
      Table: {
        rows: [
          { cells: [{ content: [{ Text: run('a') }] }, { content: [] }] },
          { cells: [{ content: [] }, { content: [{ Text: run('d') }] }] },
        ],
      },
    };
    expect(JSON.stringify(block)).toBe(
      `{"Table":{"rows":[{"cells":[{"content":[${plainRun('a')}]},{"content":[]}]},{"cells":[{"content":[]},{"content":[${plainRun('d')}]}]}]}}`,
    );
  });

  it('carries both quote styles with the attribution and both bodies', () => {
    const headers = [{ label: 'From', value: 'Alice <a@x.test>' }];
    expect(JSON.stringify(quoteBlock('Indented'))).toBe(
      '{"Quote":{"style":"Indented","attribution":{"line":"On 30 Jun 2026 at 14:03, Alice wrote:","headers":[]},"body_html":"<p>Original</p>","body_plain":"Original"}}',
    );
    const withHeader: Block = {
      Quote: {
        style: 'LineAndHeader',
        attribution: { line: '', headers },
        body_html: '<p>Original</p>',
        body_plain: 'Original',
      },
    };
    expect(JSON.stringify(withHeader)).toBe(
      '{"Quote":{"style":"LineAndHeader","attribution":{"line":"","headers":[{"label":"From","value":"Alice <a@x.test>"}]},"body_html":"<p>Original</p>","body_plain":"Original"}}',
    );
  });

  it('carries a signature block', () => {
    expect(JSON.stringify(signatureBlock())).toBe(
      '{"Signature":{"body_html":"<p>Ada Lovelace</p>","body_plain":"Ada Lovelace"}}',
    );
  });

  it('carries one inline disposition attachment and one regular file', () => {
    const doc: ComposerDocument = {
      blocks: [{ Paragraph: { content: [] } }],
      attachments: [
        inlineAttachment(),
        {
          id: 'att-2',
          blob: null,
          file_name: 'notes.pdf',
          media_type: 'application/pdf',
          size: null,
          disposition: 'Attachment',
          data_url: null,
        },
      ],
    };
    expect(JSON.stringify(doc.attachments)).toBe(
      '[{"id":"att-1","blob":"blob-7","file_name":"chart.png","media_type":"image/png","size":2048,"disposition":{"Inline":{"cid":"chart@astra"}},"data_url":null},{"id":"att-2","blob":null,"file_name":"notes.pdf","media_type":"application/pdf","size":null,"disposition":"Attachment","data_url":null}]',
    );
  });
});

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

  it('retains tables, quotes, signatures, images and marked runs', () => {
    const styled: Block = { Paragraph: { content: [{ Text: { ...run('big'), font_size: 'Large' } }] } };
    const withImage: Block = {
      Paragraph: { content: [{ Image: { attachment_id: 'att-1', alt_text: 'chart', width_px: 320 } }] },
    };
    const seeded = documentToEditor({
      blocks: [styled, tableBlock(), quoteBlock(), signatureBlock(), withImage],
      attachments: [],
    });

    expect(seeded.content).toEqual([]);
    expect(seeded.retained.map((entry) => entry.index)).toEqual([0, 1, 2, 3, 4]);
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
