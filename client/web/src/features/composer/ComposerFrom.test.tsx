// SPDX-License-Identifier: GPL-3.0-only
// The pane's own wiring for the From picker and the send-time link
// sentence: the picker draws the account rows with the default send
// account first, a choice crosses through the store's swap rule, and the
// degradation sentence renders under the toolbar, never blocking the send.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
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

vi.mock('./Editor', () => ({
  Editor: (_props: { initialContent: JSONContent[]; onChange: (doc: JSONContent) => void }) => {
    return <div data-testid="editor-stub" />;
  },
  insertAtSelection: () => undefined,
}));

import { useAccountStore } from '@/stores/accountStore';
import { useSettingsStore } from '@/features/settings/settingsStore';

import { ComposerPane } from './Composer';
import { useComposerStore } from './composerStore';
import { documentToEditor } from './documentWire';

function resetStores(): void {
  useAccountStore.setState({
    accounts: [{ accountId: 'work', displayName: 'Work', address: 'me@work.example' }],
  });
  useSettingsStore.setState({ settings: { ...useSettingsStore.getState().settings, default_send_account: undefined } });
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
  const account = useAccountStore.getState().accounts[0];
  if (account === undefined) {
    throw new Error('the picker test needs an account');
  }
  act(() => {
    useComposerStore.getState().newMessage({ id: account.accountId, label: account.displayName });
  });
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  resetStores();
});

afterEach(cleanup);

describe('the pane from picker and link sentence', () => {
  beforeEach(() => {
    useAccountStore.setState({
      accounts: [
        { accountId: 'work', displayName: 'Work', address: 'me@work.example' },
        { accountId: 'home', displayName: 'Home', address: 'me@home.example' },
      ],
    });
    useSettingsStore.setState({
      settings: { ...useSettingsStore.getState().settings, default_send_account: 'home' },
    });
  });

  it('draws the account rows with the default send account first', () => {
    newMessage();

    render(<ComposerPane />);

    const from = screen.getByRole('combobox', { name: 'From' }) as HTMLSelectElement;
    expect(Array.from(from.querySelectorAll('option')).map((option) => option.value)).toEqual([
      'home',
      'work',
    ]);
    expect(from.value).toBe('work');
  });

  it('carries a choice through the store', () => {
    newMessage();
    render(<ComposerPane />);

    act(() => {
      fireEvent.change(screen.getByRole('combobox', { name: 'From' }), {
        target: { value: 'home' },
      });
    });

    expect(useComposerStore.getState().from).toBe('home');
    expect(useComposerStore.getState().fromLabel).toBe('Home');
  });

  it('renders the degradation sentence under the toolbar', async () => {
    ipc.dispatch.mockRejectedValue(new Error('the queue refused the send'));
    newMessage();
    act(() => {
      useComposerStore.getState().setField('to', 'ada@example.test');
      useComposerStore.getState().setEditorContent([
        {
          type: 'paragraph',
          content: [
            {
              type: 'text',
              marks: [{ type: 'link', attrs: { href: 'javascript:alert(1)' } }],
              text: 'run',
            },
          ],
        },
      ]);
      useComposerStore.getState().submit();
    });
    await act(async () => {
      await Promise.resolve();
    });

    render(<ComposerPane />);

    const sentence = screen.getByRole('status');
    expect(sentence.textContent).toContain('javascript:alert(1)');
    useComposerStore.getState().closePane();
  });
});
