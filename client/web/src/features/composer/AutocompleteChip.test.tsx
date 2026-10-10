// SPDX-License-Identifier: GPL-3.0-only
// The autocomplete chip: the ask fires two seconds after typing stops and
// never before, each keystroke restarts the window, both halves of the
// cursor's context cross (the trailing half only when the cursor sits
// inside text), an unchanged context is skipped, an empty answer shows
// nothing, a stale answer that settles after typing resumed is dropped,
// the insert never happens without its gesture (not on render, not on a
// re-render), Tab and click both insert and clear the chip, and a dismiss
// hides it for the rest of the session.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Editor as CoreEditor } from '@tiptap/core';

const ai = vi.hoisted(() => ({
  composeAutocomplete: vi.fn(),
}));

vi.mock('@/services/ai', () => ai);

import { AutocompleteChip } from './AutocompleteChip';
import { createComposerEditor } from './Editor';

/** A bare editor over one paragraph, with the caret named or at the end. */
function editorOver(text: string, caret?: number): CoreEditor {
  const editor = createComposerEditor([
    { type: 'paragraph', content: text === '' ? [] : [{ type: 'text', text }] },
  ]);
  editor.commands.setTextSelection(caret ?? editor.state.doc.content.size);
  return editor;
}

/** Types at the caret (or a named inline position), arming the chip. */
function type(editor: CoreEditor, text: string, at?: number): void {
  // An inline text node, not a string: a string would parse as block level
  // HTML and split the paragraph instead of typing into it.
  act(() => {
    editor.commands.insertContentAt(at ?? editor.state.selection.from, { type: 'text', text });
  });
}

/** Edits the editor's text the same keystroke path a correction takes. */
function edit(run: () => void): void {
  act(run);
}

/** Runs the idle window out, inside act, flushing the ask's microtasks. */
async function idle(ms = 2000): Promise<void> {
  await act(async () => {
    vi.advanceTimersByTime(ms);
  });
}

async function flush(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  ai.composeAutocomplete.mockReset();
  // The real service always answers a promise; the tests that care about
  // the answer override this with their own.
  ai.composeAutocomplete.mockResolvedValue('');
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('the idle ask', () => {
  it('fires two seconds after typing stops, the cursor\'s text before it', async () => {
    const editor = editorOver('Hello');
    const onInsert = vi.fn();
    render(<AutocompleteChip editor={editor} container={{ current: null }} onInsert={onInsert} />);

    type(editor, ' there');
    await idle(1999);
    expect(ai.composeAutocomplete).not.toHaveBeenCalled();

    await idle(1);
    expect(ai.composeAutocomplete).toHaveBeenCalledTimes(1);
    expect(ai.composeAutocomplete).toHaveBeenCalledWith('Hello there');
    editor.destroy();
  });

  it('crosses the trailing half too when the cursor sits inside text', async () => {
    const editor = editorOver('Hello world');
    const onInsert = vi.fn();
    render(<AutocompleteChip editor={editor} container={{ current: null }} onInsert={onInsert} />);

    // Insert before 'world' so the caret lands between the two halves.
    type(editor, ' brave', 6);
    await idle();

    expect(ai.composeAutocomplete).toHaveBeenCalledTimes(1);
    expect(ai.composeAutocomplete).toHaveBeenCalledWith('Hello brave', ' world');
    editor.destroy();
  });

  it('restarts the window on each keystroke', async () => {
    const editor = editorOver('Hello');
    const onInsert = vi.fn();
    render(<AutocompleteChip editor={editor} container={{ current: null }} onInsert={onInsert} />);

    type(editor, ' one');
    await idle(1500);
    type(editor, ' two');
    await idle(1500);
    expect(ai.composeAutocomplete).not.toHaveBeenCalled();

    await idle(500);
    expect(ai.composeAutocomplete).toHaveBeenCalledTimes(1);
    expect(ai.composeAutocomplete).toHaveBeenCalledWith('Hello one two');
    editor.destroy();
  });

  it('skips the ask when the text around the cursor is unchanged', async () => {
    const editor = editorOver('Hello');
    const onInsert = vi.fn();
    render(<AutocompleteChip editor={editor} container={{ current: null }} onInsert={onInsert} />);

    type(editor, ' there');
    await idle();
    expect(ai.composeAutocomplete).toHaveBeenCalledTimes(1);
    expect(ai.composeAutocomplete).toHaveBeenCalledWith('Hello there');

    // A correction pair inside one idle window leaves the text back at what
    // already asked, so the window that closes over it spends no call.
    type(editor, '!');
    edit(() => {
      editor.commands.deleteRange({ from: 12, to: 13 });
    });
    await idle();

    expect(ai.composeAutocomplete).toHaveBeenCalledTimes(1);
    editor.destroy();
  });

  it('makes no ask over an empty editor', async () => {
    const editor = editorOver('H');
    const onInsert = vi.fn();
    render(<AutocompleteChip editor={editor} container={{ current: null }} onInsert={onInsert} />);

    edit(() => {
      editor.commands.deleteRange({ from: 1, to: 2 });
    });
    await idle();

    expect(ai.composeAutocomplete).not.toHaveBeenCalled();
    editor.destroy();
  });

  it('shows nothing for an empty answer', async () => {
    ai.composeAutocomplete.mockResolvedValue('');
    const editor = editorOver('Hello');
    const onInsert = vi.fn();
    const { container } = render(
      <AutocompleteChip editor={editor} container={{ current: null }} onInsert={onInsert} />,
    );

    type(editor, ' there');
    await idle();

    expect(container.firstChild).toBeNull();
    editor.destroy();
  });

  it('drops a stale answer that settles after typing resumed', async () => {
    let resolveAsk: (text: string) => void = () => undefined;
    ai.composeAutocomplete.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolveAsk = resolve;
        }),
    );
    const editor = editorOver('Hello');
    const onInsert = vi.fn();
    const { container } = render(
      <AutocompleteChip editor={editor} container={{ current: null }} onInsert={onInsert} />,
    );

    type(editor, ' there');
    await idle();
    expect(ai.composeAutocomplete).toHaveBeenCalledTimes(1);
    type(editor, ' more');
    resolveAsk(' a stale suggestion');
    await flush();

    expect(container.firstChild).toBeNull();
    expect(onInsert).not.toHaveBeenCalled();
    editor.destroy();
  });
});

describe('the gesture-only insert', () => {
  /** Mounts the chip over its editor with the answer already armed. */
  function chipOver(editor: CoreEditor) {
    const onInsert = vi.fn();
    const view = render(
      <AutocompleteChip editor={editor} container={{ current: null }} onInsert={onInsert} />,
    );
    return { onInsert, ...view };
  }

  it('never inserts on its own, across a re-render included', async () => {
    ai.composeAutocomplete.mockResolvedValue(' world');
    const editor = editorOver('Hello');
    const { onInsert, rerender } = chipOver(editor);

    type(editor, ' there');
    await idle();
    expect(screen.getByText('world')).toBeTruthy();
    expect(onInsert).not.toHaveBeenCalled();

    rerender(
      <AutocompleteChip editor={editor} container={{ current: null }} onInsert={onInsert} />,
    );
    await flush();

    expect(screen.getByText('world')).toBeTruthy();
    expect(onInsert).not.toHaveBeenCalled();
    editor.destroy();
  });

  it('inserts on Tab and clears the chip', async () => {
    ai.composeAutocomplete.mockResolvedValue(' world');
    const editor = editorOver('Hello');
    const { onInsert } = chipOver(editor);

    type(editor, ' there');
    await idle();
    expect(screen.getByText('world')).toBeTruthy();
    expect(onInsert).not.toHaveBeenCalled();

    fireEvent.keyDown(document, { key: 'Tab' });

    expect(onInsert).toHaveBeenCalledWith(' world');
    expect(screen.queryByText('world')).toBeNull();
    editor.destroy();
  });

  it('inserts on click and clears the chip', async () => {
    ai.composeAutocomplete.mockResolvedValue(' world');
    const editor = editorOver('Hello');
    const { onInsert } = chipOver(editor);

    type(editor, ' there');
    await idle();
    expect(screen.getByText('world')).toBeTruthy();
    expect(onInsert).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText('world'));

    expect(onInsert).toHaveBeenCalledWith(' world');
    expect(screen.queryByText('world')).toBeNull();
    editor.destroy();
  });
});

describe('the dismiss', () => {
  it('hides the chip for the rest of the session', async () => {
    const editor = editorOver('Hello');
    const onInsert = vi.fn();
    const { container } = render(
      <AutocompleteChip editor={editor} container={{ current: null }} onInsert={onInsert} />,
    );
    ai.composeAutocomplete.mockResolvedValue(' world');
    type(editor, ' there');
    await idle();
    expect(screen.getByText('world')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Dismiss suggestion' }));
    expect(screen.queryByText('world')).toBeNull();

    type(editor, ' again');
    ai.composeAutocomplete.mockClear();
    await idle();

    expect(ai.composeAutocomplete).not.toHaveBeenCalled();
    expect(container.firstChild).toBeNull();
    expect(onInsert).not.toHaveBeenCalled();
    editor.destroy();
  });
});
