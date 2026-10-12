// SPDX-License-Identifier: GPL-3.0-only
// The composer's opening: the seeded recipient fields normalise their
// opening value so every supplied address is a pill with nothing in
// progress and dirtiness compares against the normalised value, and the
// caret opens where the work starts, decided by the one predicate over the
// composition's own request and To.
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
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

const editorStub = vi.hoisted(() => ({ commands: { focus: vi.fn() } }));

vi.mock('./Editor', () => ({
  Editor: (props: {
    initialContent: JSONContent[];
    onChange: (doc: JSONContent) => void;
    onReady?: (editor: { commands: { focus: () => void } } | null) => void;
  }) => {
    props.onReady?.(editorStub);
    return <div data-testid="editor-stub" />;
  },
  insertAtSelection: () => undefined,
}));

import type { ComposeRequestSnapshot } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';

import { ComposerPane } from './Composer';
import { opensInBody, useComposerStore } from './composerStore';
import { documentToEditor } from './documentWire';

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
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
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
  editorStub.commands.focus.mockReset();
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

  it('places the caret in the body when the composer was opened already addressed', () => {
    act(() => {
      useComposerStore.getState().openRequest(pendingRequest, 'Work');
    });
    render(<ComposerPane />);

    expect(editorStub.commands.focus).toHaveBeenCalled();
    expect(document.activeElement).not.toBe(screen.getByRole('textbox', { name: 'To' }));
  });
});
