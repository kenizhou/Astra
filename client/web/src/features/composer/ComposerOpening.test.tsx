// SPDX-License-Identifier: GPL-3.0-only
// The composer's opening: the seeded recipient fields normalise their
// opening value so every supplied address is a pill with nothing in
// progress and dirtiness compares against the normalised value, and the
// caret opens where the work starts, decided by the one predicate over the
// composition's own request and To.
import { act, cleanup, render, screen } from '@testing-library/react';
import { useEffect } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Editor as CoreEditor } from '@tiptap/core';
import type { JSONContent } from '@tiptap/react';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

const editorStub = vi.hoisted(() => ({ focus: vi.fn() }));

vi.mock('./Editor', async (importOriginal) => {
  const original = await importOriginal<typeof import('./Editor')>();
  return {
    ...original,
    Editor: (props: {
      initialContent: JSONContent[];
      onChange: (doc: JSONContent) => void;
      onReady?: (editor: CoreEditor | null) => void;
    }) => {
      // The real editor hands its instance over in an effect; the stub
      // mirrors that over a bare editor from the same trimmed extension set,
      // so the pane's toolbar can subscribe to it. The focus command reaches
      // the view through a fresh commands object per access, so the spy
      // stands on the view itself.
      useEffect(() => {
        const editor = original.createComposerEditor(props.initialContent);
        editor.view.focus = () => editorStub.focus();
        props.onReady?.(editor);
        return () => {
          props.onReady?.(null);
          editor.destroy();
        };
        // One editor per mount, like the real Editor's useEditor.
        // eslint-disable-next-line react-hooks/exhaustive-deps
      }, []);
      return <div data-testid="editor-stub" />;
    },
  };
});

import type { ComposeRequestSnapshot } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';

import { ComposerPane } from './Composer';
import { opensInBody, useComposerStore } from './composerStore';
import { quoteOrdered } from './composerOpening';
import { documentToEditor } from './documentWire';
import type { Block, ComposerDocument } from '@/services/composerDocument';

/** A pending request, standing in the store while the composer is open. */
const pendingRequest: ComposeRequestSnapshot = {
  kind: 'reply',
  from: 'work',
  to: 'ada@example.test',
  cc: '',
  subject: 'Re: Quarterly report',
  in_reply_to: 'orig-1@server.test',
  document: { blocks: [], attachments: [] },
};

function resetStores(): void {
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work', address: 'me@work.example' }] });
  useComposeRequestStore.setState({ snapshot: null });
  useComposerStore.setState({
    open: false,
    resumeAttempted: true,
    composition: null,
    from: '',
    fromLabel: '',
    to: '',
    cc: '',
    bcc: '',
    subject: '',
    editor: documentToEditor({ blocks: [{ Paragraph: { content: [] } }], attachments: [] }),
    savedFingerprint: null,
    request: null,
  });
}

/** Opens a composition the way the New message action does. */
function newMessage(): void {
  act(() => {
    useComposerStore
      .getState()
      .newMessage({ id: 'work', label: 'Work' });
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  editorStub.focus.mockReset();
  resetStores();
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('the seeded normalisation', () => {
  it('normalises the seeded recipients so every address is a pill, nothing in progress', () => {
    act(() => {
      useComposerStore.getState().hydrate(
        {
          account: 'work',
          to: ' you@test.local,them@test.local ,',
          cc: '',
          bcc: '',
          subject: 'Later',
          body_text: '',
        },
        { id: 'work', label: 'Work' },
        'compose-11',
      );
    });
    render(<ComposerPane />);

    const input = screen.getByRole('textbox', { name: 'To' }) as HTMLInputElement;
    expect(useComposerStore.getState().to).toBe('you@test.local, them@test.local');
    expect(input.value).toBe('you@test.local, them@test.local');
    expect(screen.getByRole('button', { name: 'Remove you@test.local' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Remove them@test.local' })).toBeTruthy();
  });

  it('compares dirtiness against the normalised opening value', () => {
    act(() => {
      useComposerStore.getState().hydrate(
        {
          account: 'work',
          to: ' you@test.local , them@test.local ',
          cc: '',
          bcc: '',
          subject: 'Later',
          body_text: 'Half written',
        },
        { id: 'work', label: 'Work' },
        'compose-12',
      );
    });
    render(<ComposerPane />);

    act(() => vi.advanceTimersByTime(120_000));

    expect(ipc.dispatch).not.toHaveBeenCalled();
  });
});

describe('the caret predicate', () => {
  it('opens in the body when the composer was opened already addressed', () => {
    expect(opensInBody({ kind: 'reply', inReplyTo: 'orig-1@server.test' }, '')).toBe(true);
    expect(opensInBody(null, 'you@test.local')).toBe(true);
  });

  it('opens in To on a new message', () => {
    expect(opensInBody(null, '')).toBe(false);
  });

  it('places the caret in To on a new message', () => {
    newMessage();
    render(<ComposerPane />);

    expect(document.activeElement).toBe(screen.getByRole('textbox', { name: 'To' }));
  });

  it('places the caret in the body when the composer was opened already addressed', async () => {
    act(() => {
      useComposerStore.getState().openRequest(pendingRequest, 'Work');
    });
    render(<ComposerPane />);

    // The real focus command lands on the view inside a requestAnimationFrame,
    // which the fake timers hold until advanced.
    await act(async () => {
      vi.advanceTimersByTime(100);
    });

    expect(editorStub.focus).toHaveBeenCalled();
    expect(document.activeElement).not.toBe(screen.getByRole('textbox', { name: 'To' }));
  });
});

describe('the seeded order against the quote setting', () => {
  /** The core-built seeded reply: the typed words, the quote, the seeded
   *  signature under it, the order the seeding rule names. */
  const seeded: ComposerDocument = {
    blocks: [
      { Paragraph: { content: [] } },
      {
        Quote: {
          style: 'Indented',
          attribution: { line: '', headers: [] },
          body_html: '',
          body_plain: '',
        },
      },
      { Signature: { body_html: '<p>Ada</p>', body_plain: 'Ada' } },
    ],
    attachments: [],
  };
  const shapeOf = (block: Block): string => Object.keys(block)[0]!;

  it('keeps the signature below the typed words for the below-quote setting', () => {
    const ordered = quoteOrdered(seeded, true);
    expect(ordered.blocks.map(shapeOf)).toEqual(['Paragraph', 'Quote', 'Signature']);
  });

  it('keeps the signature below the typed words when the quote lifts to the top', () => {
    const ordered = quoteOrdered(seeded, false);
    expect(ordered.blocks.map(shapeOf)).toEqual(['Quote', 'Paragraph', 'Signature']);
  });
});
