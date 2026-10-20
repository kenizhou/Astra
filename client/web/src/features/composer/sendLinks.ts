// SPDX-License-Identifier: GPL-3.0-only
// The send-time link check, the client mirror of the core's own allowlist
// (astra-composer's link.rs): http, https, and mailto only, nothing longer
// than 2048 bytes, no whitespace or control characters, and something
// after the scheme. Before a submit the document's link targets are
// validated against it and any that degrade are named in one sentence
// under the toolbar. The sentence never blocks the send: the core's
// degradation to plain text stays the send's own floor.

import type { Block, ComposerDocument, InlineContent, List, TableCell } from '@/services/ipc';

/** The schemes a link may carry; the core's own three. */
export const LINK_SCHEMES: readonly string[] = ['http', 'https', 'mailto'];

/** The longest link the composer sends. */
export const MAX_LINK_LEN = 2048;

/** The lowercased scheme, when the target is in `scheme:` form. */
function schemeOf(target: string): string | null {
  const colon = target.indexOf(':');
  if (colon <= 0) {
    return null;
  }
  const scheme = target.slice(0, colon);
  return /^[a-z][a-z0-9+.-]*$/i.test(scheme) ? scheme.toLowerCase() : null;
}

/** Whether one target survives the send as a link. */
export function linkSurvivesSend(target: string): boolean {
  const value = target.trim();
  // The length and the control refusal mirror the core's own arithmetic:
  // bytes, not UTF-16 units, and every control character including the C1
  // range, so the sentence names everything the core will degrade.
  if (
    byteLength(value) > MAX_LINK_LEN ||
    /[\s]/.test(value) ||
    /[\u0000-\u001f\u007f-\u009f]/.test(value)
  ) {
    return false;
  }
  const scheme = schemeOf(value);
  if (scheme === null || !LINK_SCHEMES.includes(scheme)) {
    return false;
  }
  const rest = value.slice(value.indexOf(':') + 1).replace(/^\/+/, '');
  return rest !== '';
}

/** A string's length in UTF-8 bytes, the unit the core's bound counts. */
function byteLength(value: string): number {
  return new TextEncoder().encode(value).length;
}

/** The targets one block's inline content carries, in order. */
function targetsOf(content: InlineContent[] | undefined, out: string[]): void {
  for (const inline of content ?? []) {
    if ('Text' in inline && inline.Text.link !== null && !out.includes(inline.Text.link)) {
      out.push(inline.Text.link);
    }
  }
}

/** The link targets a list carries, through its nested children. */
function listTargets(list: List, out: string[]): void {
  for (const item of list.items) {
    targetsOf(item.content, out);
    if (item.child !== null) {
      listTargets(item.child, out);
    }
  }
}

/** The link targets a table carries, cell by cell. */
function tableTargets(rows: { cells: TableCell[] }[], out: string[]): void {
  for (const row of rows) {
    for (const cell of row.cells) {
      targetsOf(cell.content, out);
    }
  }
}

/**
 * The document's link targets the core would degrade to plain text at the
 * send, in document order, each named once.
 */
export function degradingLinkTargets(document: ComposerDocument): string[] {
  const out: string[] = [];
  const walk = (blocks: Block[]): void => {
    for (const block of blocks) {
      if ('Paragraph' in block) {
        targetsOf(block.Paragraph.content, out);
      } else if ('List' in block) {
        listTargets(block.List, out);
      } else if ('Table' in block) {
        tableTargets(block.Table.rows, out);
      }
    }
  };
  walk(document.blocks);
  return out.filter((target) => !linkSurvivesSend(target));
}

/**
 * The sentence the degradation renders, or null when every target stands.
 */
export function degradationSentence(targets: string[]): string | null {
  if (targets.length === 0) {
    return null;
  }
  return `These links cannot ride the message and will send as plain text: ${targets.join(', ')}`;
}
