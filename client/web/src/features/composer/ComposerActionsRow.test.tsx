// SPDX-License-Identifier: GPL-3.0-only
// The composer actions row: the primary Send rides the store's one submit
// path and stands gated until a recipient stands, Discard removes the stored
// draft and closes the composition, and Close (the pop-out window's button,
// absent from the strip) hands the whole draft over as a final save before
// closing.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

import { useAccountStore } from '@/stores/accountStore';
import { useComposeRequestStore } from '@/stores/composeRequestStore';

import { ComposerActionsRow } from './ComposerActionsRow';
import { useComposerStore } from './composerStore';
import { documentToEditor } from './documentWire';

function resetStores(): void {
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useComposeRequestStore.setState({ snapshot: null });
  useComposerStore.setState({
    open: false,
    inlineExpected: false,
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

/** Opens a plain composition and answers its id. */
function newMessage(): string {
  act(() => {
    useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
  });
  return useComposerStore.getState().composition as string;
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  resetStores();
});

afterEach(cleanup);

describe('the send button', () => {
  it('stands gated until a recipient stands', () => {
    newMessage();
    render(<ComposerActionsRow />);

    const send = screen.getByRole('button', { name: 'Send' });
    expect(send.hasAttribute('disabled')).toBe(true);

    act(() => {
      useComposerStore.getState().setField('to', 'you@test.local');
    });
    expect(screen.getByRole('button', { name: 'Send' }).hasAttribute('disabled')).toBe(false);
  });

  it('submits the current document through the store path', async () => {
    newMessage();
    render(<ComposerActionsRow />);
    act(() => {
      const store = useComposerStore.getState();
      store.setField('to', 'you@test.local');
      store.setEditorContent([
        { type: 'paragraph', content: [{ type: 'text', text: 'Ready' }] },
      ]);
    });

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Send' }));
      await Promise.resolve();
    });

    const submit = ipc.dispatch.mock.calls[0]?.[0];
    expect(JSON.stringify(submit)).toContain('"submit_rich_mail"');
    expect(JSON.stringify(submit)).toContain('"to":"you@test.local"');
    expect(useComposerStore.getState().open).toBe(false);
  });
});

describe('the right-hand actions', () => {
  it('offers the snippet picker seam', () => {
    newMessage();
    render(<ComposerActionsRow />);
    expect(screen.getByRole('button', { name: 'Snippets' })).toBeTruthy();
  });

  it('discards the stored draft and closes the composition', () => {
    const composition = newMessage();
    render(<ComposerActionsRow />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      `{"drafts":{"discard":{"composition":"${composition}"}}}`,
    );
    expect(useComposerStore.getState().open).toBe(false);
  });

  it('stands without a Close button unless the window asks for one', () => {
    newMessage();
    render(<ComposerActionsRow />);
    expect(screen.queryByRole('button', { name: 'Close composer' })).toBeNull();
  });

  it('closes through a final save with then_close when the window asks', () => {
    const composition = newMessage();
    render(<ComposerActionsRow showClose />);
    act(() => {
      useComposerStore.getState().setField('subject', 'Later');
    });

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Close composer' }));
    });

    const save = JSON.stringify(ipc.dispatch.mock.calls[0]?.[0]);
    expect(save).toContain(`"composition":"${composition}"`);
    expect(save).toContain('"then_close":true');
    expect(useComposerStore.getState().open).toBe(false);
  });
});
