// SPDX-License-Identifier: GPL-3.0-only
// The document wire: TipTap's editor JSON to and from the ComposerDocument
// the core stores, both ways deterministic. Blocks the trimmed editor
// cannot produce (tables, quotes, signatures, inline images, runs with
// marks it cannot show) never enter the editor: they are retained verbatim
// and spliced back at their original positions, so nothing crosses silently
// altered. A shape the wire cannot carry at all is rejected loudly with a
// typed error, never dropped.

import type { JSONContent } from '@tiptap/react';

import type {
  Block,
  ComposerDocument,
  DraftAttachment,
  InlineContent,
  List,
  ListItem,
  TextRun,
} from '@/services/ipc';

/** The loud reject: a shape the wire cannot carry without losing content. */
export class DocumentWireError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'DocumentWireError';
  }
}

/** One block held out of the editor, with the block index it came from. */
export interface RetainedBlock {
  index: number;
  block: Block;
}

/** The editor's share of one composer document. */
export interface EditorDocument {
  /** The editable blocks as TipTap doc content, in document order. */
  content: JSONContent[];
  /** The blocks the trimmed editor cannot produce, untouched, in order. */
  retained: RetainedBlock[];
  /** The document's attachments, carried across untouched. */
  attachments: DraftAttachment[];
}

/** A run whose marks are all ones the trimmed editor can show and set. */
function runIsEditable(run: TextRun): boolean {
  return run.font_size === null && run.color === null && run.highlight === null;
}

/** Inline content the editor can hold: text runs with its own mark set. */
function inlinesAreEditable(content: InlineContent[]): boolean {
  return content.every((inline) => 'Text' in inline && runIsEditable(inline.Text));
}

/** A list the editor can hold: every item editable and every child too. */
function listIsEditable(list: List): boolean {
  return (
    list.items.length > 0 &&
    list.items.every(
      (item) =>
        inlinesAreEditable(item.content) && (item.child === null || listIsEditable(item.child)),
    )
  );
}

/** One TipTap mark as the wire builds it. */
interface EditorMark {
  type: string;
  attrs?: Record<string, unknown>;
}

/** The TipTap marks a run carries, in a fixed order for determinism. */
function runMarks(run: TextRun): EditorMark[] {
  const marks: EditorMark[] = [];
  if (run.bold) {
    marks.push({ type: 'bold' });
  }
  if (run.italic) {
    marks.push({ type: 'italic' });
  }
  if (run.underline) {
    marks.push({ type: 'underline' });
  }
  if (run.link !== null) {
    marks.push({ type: 'link', attrs: { href: run.link } });
  }
  return marks;
}

/** One run as a TipTap text node. */
function runNode(inline: InlineContent): JSONContent | null {
  if (!('Text' in inline)) {
    return null;
  }
  const run = inline.Text;
  const node: JSONContent = { type: 'text', text: run.text };
  const marks = runMarks(run);
  if (marks.length > 0) {
    node.marks = marks;
  }
  return node;
}

/** Inline content as TipTap nodes; an empty array yields no content key. */
function inlineNodes(content: InlineContent[]): JSONContent[] | undefined {
  const nodes = content.map(runNode).filter((node): node is JSONContent => node !== null);
  return nodes.length > 0 ? nodes : undefined;
}

/** A paragraph block as a TipTap paragraph node. */
function paragraphNode(content: InlineContent[]): JSONContent {
  const nodes = inlineNodes(content);
  return nodes !== undefined ? { type: 'paragraph', content: nodes } : { type: 'paragraph' };
}

/** A list as a TipTap list node, recursively through the nested children. */
function listNode(list: List): JSONContent {
  return {
    type: list.kind === 'Bullet' ? 'bulletList' : 'orderedList',
    content: list.items.map((item) => ({
      type: 'listItem',
      content: [
        paragraphNode(item.content),
        ...(item.child !== null ? [listNode(item.child)] : []),
      ],
    })),
  };
}

/**
 * Splits one wire document into the editor's share and what it retains. A
 * block the trimmed editor cannot produce is retained with its original
 * index so the way back splices it where it stood.
 */
export function documentToEditor(doc: ComposerDocument): EditorDocument {
  const content: JSONContent[] = [];
  const retained: RetainedBlock[] = [];

  doc.blocks.forEach((block, index) => {
    if ('Paragraph' in block) {
      if (inlinesAreEditable(block.Paragraph.content)) {
        content.push(paragraphNode(block.Paragraph.content));
        return;
      }
    } else if ('List' in block) {
      if (listIsEditable(block.List)) {
        content.push(listNode(block.List));
        return;
      }
    }
    retained.push({ index, block });
  });

  return { content, retained, attachments: doc.attachments };
}

/** The marks of one TipTap text node as the wire spells them. */
function marksOf(node: JSONContent): Pick<TextRun, 'bold' | 'italic' | 'underline' | 'link'> {
  let bold = false;
  let italic = false;
  let underline = false;
  let link: string | null = null;
  for (const mark of node.marks ?? []) {
    if (mark.type === 'bold') {
      bold = true;
    } else if (mark.type === 'italic') {
      italic = true;
    } else if (mark.type === 'underline') {
      underline = true;
    } else if (mark.type === 'link') {
      const href = mark.attrs?.href;
      if (typeof href !== 'string') {
        throw new DocumentWireError('a link mark crossed without its target');
      }
      link = href;
    } else {
      throw new DocumentWireError(`the ${mark.type} mark is not one the wire carries`);
    }
  }
  return { bold, italic, underline, link };
}

/** Appends one text node's words, merging with a run that carries the same marks. */
function appendRun(node: JSONContent, content: InlineContent[]): void {
  const marks = marksOf(node);
  const text = node.text ?? '';
  const previous = content.at(-1);
  if (
    previous !== undefined &&
    'Text' in previous &&
    previous.Text.bold === marks.bold &&
    previous.Text.italic === marks.italic &&
    previous.Text.underline === marks.underline &&
    previous.Text.link === marks.link
  ) {
    previous.Text.text += text;
    return;
  }
  content.push({
    Text: {
      text,
      bold: marks.bold,
      italic: marks.italic,
      underline: marks.underline,
      font_size: null,
      color: null,
      highlight: null,
      link: marks.link,
    },
  });
}

/**
 * The paragraphs one editor paragraph stands for: a hard break ends the
 * line and starts the next paragraph, the way mail clients read a soft
 * break back.
 */
function paragraphsFromNode(node: JSONContent, blocks: Block[]): void {
  let content: InlineContent[] = [];
  const flush = (): void => {
    blocks.push({ Paragraph: { content } });
    content = [];
  };
  for (const child of node.content ?? []) {
    if (child.type === 'hardBreak') {
      flush();
    } else if (child.type === 'text') {
      appendRun(child, content);
    } else {
      throw new DocumentWireError(`the ${child.type} node is not one the wire carries`);
    }
  }
  flush();
}

/** One editor list as the wire's list value. */
function listFromNode(node: JSONContent, kind: List['kind']): List {
  const items = node.content ?? [];
  if (items.length === 0) {
    throw new DocumentWireError('a list crossed without any items');
  }
  return { kind, items: items.map(itemFromNode) };
}

/** One editor list item: its words, plus its nested list when it has one. */
function itemFromNode(node: JSONContent): ListItem {
  const content: InlineContent[] = [];
  let child: List | null = null;
  for (const inner of node.content ?? []) {
    if (inner.type === 'paragraph') {
      // A soft break inside a list item cannot become a second item or a
      // second block, so it stays in the words as a line break.
      for (const child2 of inner.content ?? []) {
        if (child2.type === 'hardBreak') {
          appendRun({ type: 'text', text: '\n' }, content);
        } else if (child2.type === 'text') {
          appendRun(child2, content);
        } else {
          throw new DocumentWireError(`the ${child2.type} node is not one the wire carries`);
        }
      }
    } else if (inner.type === 'bulletList' || inner.type === 'orderedList') {
      if (child !== null) {
        throw new DocumentWireError('a list item crossed with two nested lists');
      }
      child = listFromNode(inner, inner.type === 'bulletList' ? 'Bullet' : 'Ordered');
    } else {
      throw new DocumentWireError(`the ${inner.type} node is not one the wire carries`);
    }
  }
  return { content, child };
}

/** The editor's content as the document's blocks, in order. */
function blocksFromContent(content: JSONContent[]): Block[] {
  const blocks: Block[] = [];
  for (const node of content) {
    if (node.type === 'paragraph') {
      paragraphsFromNode(node, blocks);
    } else if (node.type === 'bulletList' || node.type === 'orderedList') {
      blocks.push({
        List: listFromNode(node, node.type === 'bulletList' ? 'Bullet' : 'Ordered'),
      });
    } else {
      throw new DocumentWireError(`the ${node.type} node is not one the wire carries`);
    }
  }
  return blocks;
}

/**
 * Splices the retained blocks back where they stood: each keeps its
 * original position among the blocks the editor now holds, blocks the user
 * added beyond a gap's original width land after the last retained block,
 * and a gap the user emptied closes over. Nothing retained ever moves
 * across another retained block.
 */
function spliceRetained(blocks: Block[], retained: RetainedBlock[]): Block[] {
  if (retained.length === 0) {
    return blocks;
  }
  const out: Block[] = [];
  let cursor = 0;
  let previous = -1;
  for (const { index, block } of retained) {
    const gap = Math.max(0, index - previous - 1);
    for (let k = 0; k < gap && cursor < blocks.length; k += 1) {
      const next = blocks[cursor];
      if (next === undefined) {
        break;
      }
      out.push(next);
      cursor += 1;
    }
    out.push(block);
    previous = index;
  }
  while (cursor < blocks.length) {
    const next = blocks[cursor];
    if (next === undefined) {
      break;
    }
    out.push(next);
    cursor += 1;
  }
  return out;
}

/**
 * Reads the editor's current content back into the whole document: the
 * edited blocks plus the retained ones where they stood, and the
 * attachments untouched. An editor holding only one empty paragraph is an
 * empty body, which the document spells as one empty paragraph.
 */
export function editorToDocument(editor: EditorDocument): ComposerDocument {
  const blocks = spliceRetained(blocksFromContent(editor.content), editor.retained);
  return {
    blocks: blocks.length > 0 ? blocks : [{ Paragraph: { content: [] } }],
    attachments: editor.attachments,
  };
}

/**
 * The document a plain text body opens as: one paragraph per line, the
 * resume path's seed. An empty body is one empty paragraph.
 */
export function documentFromPlainText(text: string): ComposerDocument {
  if (text === '') {
    return { blocks: [{ Paragraph: { content: [] } }], attachments: [] };
  }
  return {
    blocks: text.split('\n').map((line) => ({
      Paragraph: {
        content:
          line === ''
            ? []
            : [
                {
                  Text: {
                    text: line,
                    bold: false,
                    italic: false,
                    underline: false,
                    font_size: null,
                    color: null,
                    highlight: null,
                    link: null,
                  },
                },
              ],
      },
    })),
    attachments: [],
  };
}
