// SPDX-License-Identifier: GPL-3.0-only
// The typed-address recogniser: the words that leave the caret commit a
// delimited http, https, mailto, or www prefix as a link run, a bare domain
// never links, an undelimited prefix never links, a word already inside a
// link run is left alone, and enter commits exactly as space does.
import { describe, expect, it } from 'vitest';

import { createComposerEditor } from './Editor';
import { commitTypedLink, typedLinkTarget } from './typedLinks';

describe('the pure recogniser', () => {
  it('commits the delimited scheme prefixes', () => {
    expect(typedLinkTarget('https://example.test/report')).toBe('https://example.test/report');
    expect(typedLinkTarget('http://example.test')).toBe('http://example.test');
    expect(typedLinkTarget('mailto:ada@example.test')).toBe('mailto:ada@example.test');
  });

  it('commits a www prefix under https', () => {
    expect(typedLinkTarget('www.example.test')).toBe('https://www.example.test');
  });

  it('never links a bare domain or a bare prefix', () => {
    expect(typedLinkTarget('example.test')).toBeNull();
    expect(typedLinkTarget('ada@example.test')).toBeNull();
    expect(typedLinkTarget('http://')).toBeNull();
    expect(typedLinkTarget('www.')).toBeNull();
    expect(typedLinkTarget('')).toBeNull();
  });

  it('never links an undelimited prefix', () => {
    expect(typedLinkTarget('xhttps://example.test')).toBeNull();
    expect(typedLinkTarget('seemailto:ada@example.test')).toBeNull();
  });
});

/** A live editor holding the words, the caret parked at the paragraph end. */
function editorWith(text: string) {
  const editor = createComposerEditor([{ type: 'paragraph', content: [{ type: 'text', text }] }]);
  editor.commands.focus('end');
  return editor;
}

describe('the live commit', () => {
  it('links the delimited word before the caret', () => {
    const editor = editorWith('see https://example.test');

    expect(commitTypedLink(editor)).toBe(true);
    expect(editor.getAttributes('link').href).toBe('https://example.test');
    editor.destroy();
  });

  it('commits a mailto address and a www prefix', () => {
    const mailto = editorWith('write mailto:ada@example.test');
    expect(commitTypedLink(mailto)).toBe(true);
    expect(mailto.getAttributes('link').href).toBe('mailto:ada@example.test');
    mailto.destroy();

    const www = editorWith('browse www.example.test');
    expect(commitTypedLink(www)).toBe(true);
    expect(www.getAttributes('link').href).toBe('https://www.example.test');
    www.destroy();
  });

  it('leaves a bare domain and an undelimited prefix unlinked', () => {
    const bare = editorWith('see example.test');
    expect(commitTypedLink(bare)).toBe(false);
    expect(bare.isActive('link')).toBe(false);
    bare.destroy();

    const glued = editorWith('see xhttps://example.test');
    expect(commitTypedLink(glued)).toBe(false);
    glued.destroy();
  });

  it('never links the word the caret sits in when it is already a link', () => {
    const editor = createComposerEditor([
      {
        type: 'paragraph',
        content: [
          {
            type: 'text',
            marks: [{ type: 'link', attrs: { href: 'https://example.test/a' } }],
            text: 'https://example.test/a',
          },
          { type: 'text', text: ' and words' },
        ],
      },
    ]);
    // The caret stands at the linked run's end, inside the link.
    editor.commands.setTextSelection(1 + 'https://example.test/a'.length);

    expect(commitTypedLink(editor)).toBe(false);
    const runs = editor.getJSON().content?.[0]?.content ?? [];
    expect(runs.filter((run) => run.marks !== undefined)).toHaveLength(1);
    editor.destroy();
  });

  it('commits only the word before the caret, not the line after it', () => {
    const editor = editorWith('https://one.test then two.test');
    editor.commands.setTextSelection(1 + 'https://one.test'.length);

    expect(commitTypedLink(editor)).toBe(true);
    const runs = editor.getJSON().content?.[0]?.content ?? [];
    expect(runs[0]?.marks?.[0]?.type).toBe('link');
    expect(runs[0]?.marks?.[0]?.attrs?.href).toBe('https://one.test');
    expect(runs[1]?.marks).toBeUndefined();
    editor.destroy();
  });

  it('never commits over a range selection', () => {
    const editor = editorWith('see https://example.test now');
    // The person selected the words after the address; the keystroke that
    // fires the recogniser is theirs to spend on the selection.
    editor.commands.setTextSelection({ from: 25, to: 28 });

    expect(commitTypedLink(editor)).toBe(false);
    expect(editor.getAttributes('link').href).toBeUndefined();
    expect(editor.state.selection.empty).toBe(false);
    editor.destroy();
  });

  it('never commits a word joined by a leaf, such as a hard break', () => {
    const editor = createComposerEditor([
      {
        type: 'paragraph',
        content: [
          { type: 'text', text: 'https://one.test' },
          { type: 'hardBreak' },
          { type: 'text', text: 'tail' },
        ],
      },
    ]);
    editor.commands.focus('end');

    expect(commitTypedLink(editor)).toBe(false);
    expect(editor.getAttributes('link').href).toBeUndefined();
    editor.destroy();
  });

  it('undoes the commit as one step', () => {
    const editor = editorWith('see https://example.test');

    expect(commitTypedLink(editor)).toBe(true);
    editor.commands.undo();
    expect(editor.getText()).toBe('see https://example.test');
    expect(editor.getAttributes('link').href).toBeUndefined();
    editor.destroy();
  });
});
