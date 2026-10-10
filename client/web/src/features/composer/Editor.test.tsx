// SPDX-License-Identifier: GPL-3.0-only
// The trimmed editor: the extensions render the seeded document (paragraphs,
// marks, links and both list kinds), the contenteditable carries the body
// label, and the bare core editor built from the same extension set hands
// its JSON to the change callback. What the schema accepts is pinned by the
// document wire tests that round trip through a real editor.
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { JSONContent } from '@tiptap/react';

import { Editor, createComposerEditor } from './Editor';

afterEach(cleanup);

describe('the editor surface', () => {
  it('renders the seeded document into the contenteditable', () => {
    const initialContent: JSONContent[] = [
      {
        type: 'paragraph',
        content: [
          { type: 'text', text: 'the launch is ' },
          { type: 'text', marks: [{ type: 'bold' }], text: 'ready' },
        ],
      },
      {
        type: 'bulletList',
        content: [
          {
            type: 'listItem',
            content: [{ type: 'paragraph', content: [{ type: 'text', text: 'one' }] }],
          },
        ],
      },
    ];

    render(<Editor initialContent={initialContent} onChange={() => undefined} />);

    const body = screen.getByLabelText('Message body');
    expect(body.textContent).toContain('the launch is');
    expect(body.textContent).toContain('ready');
    expect(body.textContent).toContain('one');
    expect(body.querySelector('strong')?.textContent).toBe('ready');
    expect(body.querySelector('ul')?.textContent).toBe('one');
  });

  it('hands the document json to the change callback on an update', () => {
    const onChange = vi.fn();
    const editor = createComposerEditor([
      { type: 'paragraph', content: [{ type: 'text', text: 'hello' }] },
    ]);
    editor.on('update', ({ editor: updated }) => onChange(updated.getJSON()));

    editor.commands.insertContentAt(6, ' there');

    expect(onChange).toHaveBeenCalledTimes(1);
    const doc = onChange.mock.calls[0]?.[0];
    expect(doc).toBeDefined();
    expect(JSON.stringify(doc)).toContain('hello there');
    editor.destroy();
  });

  it('hands the live editor to the ready callback and null again on unmount', () => {
    const onReady = vi.fn();
    const { unmount } = render(
      <Editor initialContent={[{ type: 'paragraph' }]} onChange={() => undefined} onReady={onReady} />,
    );

    expect(onReady).toHaveBeenCalledTimes(1);
    expect(onReady.mock.calls[0]?.[0]?.isEmpty).toBe(true);
    unmount();
    expect(onReady).toHaveBeenLastCalledWith(null);
  });
});
