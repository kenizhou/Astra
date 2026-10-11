// SPDX-License-Identifier: GPL-3.0-only
// The snippet picker: the toolbar affordance opens the panel, the panel
// lists the library in the host's recent-first order and pulls it once
// across reopenings, the search box crosses its query debounced and a
// cleared query returns to the plain listing, a chosen row hands its body
// to the insert seam and closes, and the insert itself places plain text
// at the live editor's current selection (the TipTap discipline, pinned
// against the bare core editor: inline at the cursor, a paragraph a line,
// and a range replaced).
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Editor as CoreEditor } from '@tiptap/core';

const ai = vi.hoisted(() => ({
  snippetList: vi.fn(),
  snippetSearch: vi.fn(),
}));

vi.mock('@/services/ai', () => ai);

import { SnippetPicker } from './SnippetPicker';
import { createComposerEditor, insertAtSelection } from './Editor';
import type { SnippetView } from '@/services/ai';

/** One snippet, overridable per test. */
function snippet(over: Partial<SnippetView> = {}): SnippetView {
  return {
    id: 'snip-1',
    title: 'Signature',
    body: 'Best,\nAstra',
    created_at: 1,
    updated_at: 2,
    ...over,
  };
}

/** A library in the host's recent-first order. */
function library(): SnippetView[] {
  return [
    snippet({ id: 'snip-9', title: 'Greeting', body: 'Hi there,' }),
    snippet({ id: 'snip-1', title: 'Signature', body: 'Best,\nAstra' }),
  ];
}

/** Flushes the microtasks a click or timer leaves behind, inside act. */
async function flush(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  ai.snippetList.mockReset();
  ai.snippetSearch.mockReset();
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('the snippet picker panel', () => {
  it('opens on the toolbar affordance and lists the library recent first', async () => {
    ai.snippetList.mockResolvedValue(library());
    render(<SnippetPicker onInsert={vi.fn()} />);
    expect(screen.queryByRole('region', { name: 'Snippet picker' })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Snippets' }));
    await flush();

    const panel = screen.getByRole('region', { name: 'Snippet picker' });
    const names = panel.textContent ?? '';
    expect(names).toContain('Greeting');
    expect(names).toContain('Signature');
    expect(names.indexOf('Greeting')).toBeLessThan(names.indexOf('Signature'));
    expect(ai.snippetList).toHaveBeenCalledOnce();
    expect(
      screen.getByRole('button', { name: 'Snippets' }).getAttribute('aria-expanded'),
    ).toBe('true');
  });

  it('notes an empty library instead of an empty panel', async () => {
    ai.snippetList.mockResolvedValue([]);
    render(<SnippetPicker onInsert={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Snippets' }));
    await flush();

    expect(screen.getByText('No snippets yet.')).toBeTruthy();
  });

  it('pulls the list once across reopenings', async () => {
    ai.snippetList.mockResolvedValue(library());
    render(<SnippetPicker onInsert={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Snippets' }));
    await flush();
    expect(screen.getByText('Greeting')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Snippets' }));
    expect(screen.queryByRole('region', { name: 'Snippet picker' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Snippets' }));

    expect(screen.getByText('Greeting')).toBeTruthy();
    expect(ai.snippetList).toHaveBeenCalledOnce();
  });

  it('crosses the query debounced and shows the ranked rows', async () => {
    ai.snippetList.mockResolvedValue(library());
    ai.snippetSearch.mockResolvedValue([snippet({ id: 'snip-2', title: 'Kind regards' })]);
    render(<SnippetPicker onInsert={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Snippets' }));
    await flush();
    expect(screen.getByText('Signature')).toBeTruthy();

    fireEvent.change(screen.getByRole('textbox', { name: 'Search snippets' }), {
      target: { value: 'gree' },
    });
    await act(async () => {
      vi.advanceTimersByTime(299);
    });
    expect(ai.snippetSearch).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(1);
    });
    expect(ai.snippetSearch).toHaveBeenCalledWith('gree');
    expect(screen.getByText('Kind regards')).toBeTruthy();
    expect(screen.queryByText('Signature')).toBeNull();
  });

  it('returns to the plain listing when the query clears', async () => {
    ai.snippetList.mockResolvedValue(library());
    ai.snippetSearch.mockResolvedValue([snippet({ id: 'snip-2', title: 'Kind regards' })]);
    render(<SnippetPicker onInsert={vi.fn()} />);

    fireEvent.click(screen.getByRole('button', { name: 'Snippets' }));
    await flush();
    fireEvent.change(screen.getByRole('textbox', { name: 'Search snippets' }), {
      target: { value: 'gree' },
    });
    await act(async () => {
      vi.advanceTimersByTime(300);
    });
    expect(screen.queryByText('Signature')).toBeNull();

    fireEvent.change(screen.getByRole('textbox', { name: 'Search snippets' }), {
      target: { value: '' },
    });

    expect(screen.getByText('Signature')).toBeTruthy();
    expect(ai.snippetSearch).toHaveBeenCalledOnce();
  });

  it('hands the chosen body to the insert seam and closes the panel', async () => {
    ai.snippetList.mockResolvedValue(library());
    const onInsert = vi.fn();
    render(<SnippetPicker onInsert={onInsert} />);

    fireEvent.click(screen.getByRole('button', { name: 'Snippets' }));
    await flush();
    fireEvent.click(screen.getByText('Signature'));

    expect(onInsert).toHaveBeenCalledWith('Best,\nAstra');
    expect(screen.queryByRole('region', { name: 'Snippet picker' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Snippets' }).getAttribute('aria-expanded')).toBe(
      'false',
    );
  });
});

describe('the insert at the editor\'s current selection', () => {
  const plainEditor = (text: string): CoreEditor =>
    createComposerEditor([{ type: 'paragraph', content: [{ type: 'text', text }] }]);

  it('places an inline body at the cursor inside the current paragraph', () => {
    const editor = plainEditor('Hello ');
    editor.commands.setTextSelection(editor.state.doc.content.size);

    insertAtSelection(editor, 'world');

    expect(editor.state.doc.textBetween(0, editor.state.doc.content.size, '\n')).toBe(
      'Hello world',
    );
    editor.destroy();
  });

  it('carries each line of a multi line body as its own paragraph', () => {
    const editor = plainEditor('Hello');
    editor.commands.setTextSelection(editor.state.doc.content.size);

    insertAtSelection(editor, 'Best,\nAstra');

    expect(editor.state.doc.textBetween(0, editor.state.doc.content.size, '\n')).toBe(
      'Hello\nBest,\nAstra',
    );
    editor.destroy();
  });

  it('replaces whatever the current selection covers', () => {
    const editor = plainEditor('Hello world');
    // 'world' spans positions 7 to 12 inside the paragraph.
    editor.commands.setTextSelection({ from: 7, to: 12 });

    insertAtSelection(editor, 'there');

    expect(editor.state.doc.textBetween(0, editor.state.doc.content.size, '\n')).toBe(
      'Hello there',
    );
    editor.destroy();
  });
});
