// SPDX-License-Identifier: GPL-3.0-only
// The editor toolbar: each button drives a real editor built from the
// trimmed extension set, so what the bar offers is exactly what the schema
// accepts (bold, italic, underline, both list kinds, links, undo, redo,
// nothing else), the toggles reflect the editor's active marks, and undo
// and redo stand disabled until history holds something.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Editor as CoreEditor } from '@tiptap/core';

import { createComposerEditor } from './Editor';
import { EditorToolbar } from './EditorToolbar';

/** A real editor over the trimmed set holding one paragraph of `text`. */
function editorWith(text: string): CoreEditor {
  return createComposerEditor([{ type: 'paragraph', content: [{ type: 'text', text }] }]);
}

afterEach(cleanup);

describe('the mark toggles', () => {
  it('toggles bold on the selection and reflects the active mark', () => {
    const editor = editorWith('hello');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    const bold = screen.getByRole('button', { name: 'Bold' });
    expect(bold.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(bold);

    expect(editor.isActive('bold')).toBe(true);
    expect(JSON.stringify(editor.getJSON())).toContain('"type":"bold"');
    expect(screen.getByRole('button', { name: 'Bold' }).getAttribute('aria-pressed')).toBe('true');
    editor.destroy();
  });

  it('toggles italic and underline on the selection', () => {
    const editor = editorWith('hello');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Italic' }));
    fireEvent.click(screen.getByRole('button', { name: 'Underline' }));

    expect(editor.isActive('italic')).toBe(true);
    expect(editor.isActive('underline')).toBe(true);
    const json = JSON.stringify(editor.getJSON());
    expect(json).toContain('"type":"italic"');
    expect(json).toContain('"type":"underline"');
    editor.destroy();
  });
});

describe('the list buttons', () => {
  it('turns the paragraph into a bullet list and back', () => {
    const editor = editorWith('one');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Bullet list' }));
    expect(editor.isActive('bulletList')).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: 'Bullet list' }));
    expect(editor.isActive('bulletList')).toBe(false);
    editor.destroy();
  });

  it('turns the paragraph into an ordered list', () => {
    const editor = editorWith('one');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Numbered list' }));

    expect(editor.isActive('orderedList')).toBe(true);
    editor.destroy();
  });
});

describe('the link button', () => {
  it('sets the prompted target on the selection and unsets an active link', () => {
    const editor = editorWith('the report');
    editor.commands.selectAll();
    const prompt = vi.spyOn(window, 'prompt').mockReturnValue('https://example.test');
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Insert link' }));

    expect(editor.isActive('link')).toBe(true);
    expect(JSON.stringify(editor.getJSON())).toContain('"href":"https://example.test"');

    fireEvent.click(screen.getByRole('button', { name: 'Insert link' }));
    expect(editor.isActive('link')).toBe(false);

    prompt.mockRestore();
    editor.destroy();
  });

  it('sets nothing when the prompt is abandoned', () => {
    const editor = editorWith('the report');
    editor.commands.selectAll();
    const prompt = vi.spyOn(window, 'prompt').mockReturnValue(null);
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Insert link' }));

    expect(editor.isActive('link')).toBe(false);
    prompt.mockRestore();
    editor.destroy();
  });
});

describe('the history buttons', () => {
  it('stands disabled on a fresh editor and undoes an edit', () => {
    const editor = editorWith('hello');
    render(<EditorToolbar editor={editor} />);

    const undo = screen.getByRole('button', { name: 'Undo' });
    const redo = screen.getByRole('button', { name: 'Redo' });
    expect(undo.hasAttribute('disabled')).toBe(true);
    expect(redo.hasAttribute('disabled')).toBe(true);

    act(() => {
      // Position 6 sits at the end of 'hello', the same seam Editor.test uses.
      editor.commands.insertContentAt(6, ' world');
    });
    expect(screen.getByRole('button', { name: 'Undo' }).hasAttribute('disabled')).toBe(false);

    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(editor.getText()).toBe('hello');

    fireEvent.click(screen.getByRole('button', { name: 'Redo' }));
    expect(editor.getText()).toBe('hello world');
    editor.destroy();
  });
});

describe('the bar without an editor', () => {
  it('renders nothing until the editor stands', () => {
    const { container } = render(<EditorToolbar editor={null} />);
    expect(container.firstChild).toBeNull();
  });
});
