// SPDX-License-Identifier: GPL-3.0-only
// The typed-address recogniser: when the words leave the caret (a space or
// an enter commits), a delimited http, https, mailto, or www prefix in the
// caret's word becomes a link run. The recogniser is client-side and
// conservative: a bare domain never links, an undelimited prefix never
// links, and a word that already sits inside a link run is left alone. The
// scheme allowlist itself stays core-side; this only decides what the
// person's typing turns into a link.

import { Extension, type Editor as CoreEditor } from '@tiptap/core';

/** The prefixes that commit, as typed. */
const PREFIXES: readonly string[] = ['http://', 'https://', 'mailto:'];

/**
 * The target one word commits to, or null when the word is not a
 * delimited address. A `www.` prefix commits under https; a bare domain
 * never links.
 */
export function typedLinkTarget(word: string): string | null {
  if (word === '') {
    return null;
  }
  const lower = word.toLowerCase();
  const prefix = PREFIXES.find((candidate) => lower.startsWith(candidate));
  if (prefix !== undefined) {
    return word.length > prefix.length ? word : null;
  }
  if (lower.startsWith('www.')) {
    return word.length > 'www.'.length ? `https://${word}` : null;
  }
  return null;
}

/**
 * Commits the word before the caret as a link run when it is a delimited
 * address and not already linked. Returns whether a link was made; the
 * caret returns to where it stood, so the committing space or enter lands
 * exactly where the person typed it. A range selection never commits: the
 * recogniser reads the caret's word, and a selection the person made is
 * theirs to replace with the keystroke, not to collapse into a link.
 */
export function commitTypedLink(editor: CoreEditor): boolean {
  const { selection } = editor.state;
  if (!selection.empty) {
    return false;
  }
  const $from = selection.$from;
  const before = $from.parent.textBetween(0, $from.parentOffset, undefined, '\ufffc');
  const match = /(\S+)$/.exec(before);
  if (match === null) {
    return false;
  }
  const word = match[1]!;
  // A leaf (a hard break, an inline image) inside the word joins it with
  // the replacement character, which no address contains: refuse rather
  // than commit an href carrying it.
  if (word.includes('\ufffc')) {
    return false;
  }
  const target = typedLinkTarget(word);
  if (target === null) {
    return false;
  }
  const end = selection.from;
  const start = end - word.length;
  const link = editor.schema.marks.link;
  if (link === undefined || editor.state.doc.rangeHasMark(start, end, link)) {
    return false;
  }
  return editor
    .chain()
    .setTextSelection({ from: start, to: end })
    .setLink({ href: target })
    .setTextSelection(end)
    .run();
}

/** Binds the recogniser to the editor's space and enter keys. */
export const TypedLinkRecogniser = Extension.create({
  name: 'typedLinkRecogniser',

  addKeyboardShortcuts() {
    const commit = (): boolean => {
      // The commit inspects the words before the caret; returning false
      // lets the space or the enter land right where it was typed.
      commitTypedLink(this.editor);
      return false;
    };
    return {
      Space: commit,
      Enter: commit,
    };
  },
});
