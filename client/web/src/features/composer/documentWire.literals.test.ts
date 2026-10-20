// SPDX-License-Identifier: GPL-3.0-only
// The document wire's literal pins: the exact JSON each block variant and
// each attachment disposition crosses with, byte for byte against
// astra-composer's serde (field order is the Rust declaration order,
// optionals cross as null, variants are externally tagged CamelCase). Split
// from documentWire.test.ts so each file keeps under the line cap.
import { describe, expect, it } from 'vitest';

import type { Block, ComposerDocument, TextRun } from '@/services/ipc';

/** A plain run literal, the way a hand typed line crosses. */
function plainRun(text: string): string {
  return `{"Text":{"text":"${text}","bold":false,"italic":false,"underline":false,"font_size":null,"color":null,"highlight":null,"link":null}}`;
}

/** A run with every optional field null. */
function run(text: string): TextRun {
  return { text, bold: false, italic: false, underline: false, font_size: null, color: null, highlight: null, link: null };
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
