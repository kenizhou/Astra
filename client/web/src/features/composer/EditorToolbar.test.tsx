// SPDX-License-Identifier: GPL-3.0-only
// The editor toolbar: each button drives a real editor built from the
// trimmed extension set, so what the bar offers is exactly what the schema
// accepts (bold, italic, underline, both list kinds, the font size and the
// two colour marks, links, undo, redo, nothing else), the toggles reflect
// the editor's active marks, and undo and redo stand disabled until
// history holds something. The link button opens the in-place popover,
// never a window prompt.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
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

describe('the font size control', () => {
  it('sets one of the four steps on the selection and reflects the active one', () => {
    const editor = editorWith('hello');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    const size = screen.getByRole('combobox', { name: 'Font size' }) as HTMLSelectElement;
    expect(size.value).toBe('Normal');

    fireEvent.change(size, { target: { value: 'Large' } });

    expect(editor.getAttributes('fontSize').size).toBe('Large');
    expect((screen.getByRole('combobox', { name: 'Font size' }) as HTMLSelectElement).value).toBe(
      'Large',
    );
    editor.destroy();
  });

  it('offers exactly the four catalog steps', () => {
    const editor = editorWith('hello');
    render(<EditorToolbar editor={editor} />);

    const options = Array.from(
      screen.getByRole('combobox', { name: 'Font size' }).querySelectorAll('option'),
    ).map((option) => option.textContent);
    expect(options).toEqual(['Small', 'Normal', 'Large', 'Huge']);
    editor.destroy();
  });
});

describe('the colour pickers', () => {
  it('sets a text colour from the app palette and reflects it', () => {
    const editor = editorWith('hello');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Text colour Iris' }));

    expect(editor.getAttributes('color').color).toBe('#7a6ff0');
    expect(
      screen.getByRole('button', { name: 'Text colour Iris' }).getAttribute('aria-pressed'),
    ).toBe('true');
    editor.destroy();
  });

  it('sets a highlight colour and reflects it', () => {
    const editor = editorWith('hello');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Highlight Amber' }));

    expect(editor.getAttributes('highlight').color).toBe('#f59e0b');
    expect(
      screen.getByRole('button', { name: 'Highlight Amber' }).getAttribute('aria-pressed'),
    ).toBe('true');
    editor.destroy();
  });

  it('clears a text colour through the remove control', () => {
    const editor = editorWith('hello');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Text colour Iris' }));
    fireEvent.click(screen.getByRole('button', { name: 'Remove text colour' }));

    expect(editor.getAttributes('color').color).toBeUndefined();
    editor.destroy();
  });
});

describe('the link popover', () => {
  it('sets the target on the selection and removes it again', () => {
    const editor = editorWith('the report');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Insert link' }));

    fireEvent.change(screen.getByRole('textbox', { name: 'Link target' }), {
      target: { value: 'https://example.test' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Set link' }));

    expect(editor.isActive('link')).toBe(true);
    expect(JSON.stringify(editor.getJSON())).toContain('"href":"https://example.test"');

    fireEvent.click(screen.getByRole('button', { name: 'Insert link' }));
    fireEvent.click(screen.getByRole('button', { name: 'Remove link' }));
    expect(editor.isActive('link')).toBe(false);
    editor.destroy();
  });

  it('rewrites the display text beside the target', () => {
    const editor = editorWith('the report');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Insert link' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Link target' }), {
      target: { value: 'https://example.test' },
    });
    fireEvent.change(screen.getByRole('textbox', { name: 'Display text' }), {
      target: { value: 'the numbers' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Set link' }));

    expect(editor.getText()).toBe('the numbers');
    expect(editor.getAttributes('link').href).toBe('https://example.test');
    editor.destroy();
  });

  it('opens seeded with the link the caret sits on', () => {
    const editor = editorWith('the report');
    editor.commands.selectAll();
    editor.commands.setLink({ href: 'https://old.test' });
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Insert link' }));

    expect(
      (screen.getByRole('textbox', { name: 'Link target' }) as HTMLInputElement).value,
    ).toBe('https://old.test');
    expect(
      (screen.getByRole('textbox', { name: 'Display text' }) as HTMLInputElement).value,
    ).toBe('the report');
    editor.destroy();
  });

  it('refuses nothing but an empty target', () => {
    const editor = editorWith('the report');
    editor.commands.selectAll();
    render(<EditorToolbar editor={editor} />);

    fireEvent.click(screen.getByRole('button', { name: 'Insert link' }));
    // The scheme allowlist stays core-side: a javascript target crosses.
    fireEvent.change(screen.getByRole('textbox', { name: 'Link target' }), {
      target: { value: 'javascript:alert(1)' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Set link' }));
    expect(JSON.stringify(editor.getJSON())).toContain('"href":"javascript:alert(1)"');

    // A cleared target sets nothing and keeps the popover standing.
    fireEvent.click(screen.getByRole('button', { name: 'Insert link' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Link target' }), {
      target: { value: '' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Set link' }));
    expect(screen.getByRole('dialog', { name: 'Link' })).toBeTruthy();
    editor.destroy();
  });

  it('opens on Ctrl+K from the editor', () => {
    const editor = editorWith('the report');
    render(<EditorToolbar editor={editor} />);

    fireEvent.keyDown(editor.view.dom, { key: 'k', ctrlKey: true });

    expect(screen.getByRole('dialog', { name: 'Link' })).toBeTruthy();
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
