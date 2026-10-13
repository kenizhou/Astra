// SPDX-License-Identifier: GPL-3.0-only
// The composer status bar: the draft save state on the left (the status
// surface's word for this composition alone) and the live word and
// character counts on the right, computed from the editor document and
// moving with every edit.
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { useDraftStore } from '@/stores/draftStore';

import { ComposerStatusBar } from './ComposerStatusBar';
import { createComposerEditor } from './Editor';

beforeEach(() => {
  useDraftStore.setState({ snapshot: null });
});

afterEach(cleanup);

describe('the word counts', () => {
  it('counts the document the editor holds', () => {
    const editor = createComposerEditor([
      { type: 'paragraph', content: [{ type: 'text', text: 'one two three' }] },
    ]);
    render(<ComposerStatusBar editor={editor} composition="compose-1" />);

    expect(screen.getByText('3 words · 13 characters')).toBeTruthy();
    editor.destroy();
  });

  it('follows the edits', () => {
    const editor = createComposerEditor([
      { type: 'paragraph', content: [{ type: 'text', text: 'one' }] },
    ]);
    render(<ComposerStatusBar editor={editor} composition="compose-1" />);
    expect(screen.getByText('1 word · 3 characters')).toBeTruthy();

    act(() => {
      editor.commands.insertContentAt(4, ' two');
    });

    expect(screen.getByText('2 words · 7 characters')).toBeTruthy();
    editor.destroy();
  });

  it('counts nothing without an editor', () => {
    render(<ComposerStatusBar editor={null} composition="compose-1" />);
    expect(screen.getByText('0 words · 0 characters')).toBeTruthy();
  });
});

describe('the save state', () => {
  it('shows the status surface word for this composition alone', () => {
    render(<ComposerStatusBar editor={null} composition="compose-1" />);

    act(() => {
      useDraftStore.setState({
        snapshot: { composition: 'other-9', state: 'saving', saved_at: '' },
      });
    });
    expect(screen.queryByText('Saving draft')).toBeNull();

    act(() => {
      useDraftStore.setState({
        snapshot: { composition: 'compose-1', state: 'saving', saved_at: '' },
      });
    });
    expect(screen.getByText('Saving draft')).toBeTruthy();

    act(() => {
      useDraftStore.setState({
        snapshot: {
          composition: 'compose-1',
          state: 'saved',
          saved_at: '2026-10-09T08:30:00Z',
        },
      });
    });
    expect(screen.getByText('Saved 08:30 UTC')).toBeTruthy();
  });
});
