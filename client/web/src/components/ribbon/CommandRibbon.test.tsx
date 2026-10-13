// SPDX-License-Identifier: GPL-3.0-only
// The command ribbon: mail view only, read mode while nothing is composed
// (New Email opens a plain composition, the compose family dispatches for
// the open message with the exact payloads the wire pins, the row verbs act
// on the list's selected row with labels naming the state they leave), and
// compose mode while a composition is open (Send rides the store's submit
// path, gated the same as the pane's own Send). Under the compact
// breakpoint the buttons collapse to icon-only, labels kept on the
// tooltips.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { CommandRibbon } from './CommandRibbon';
import { useAccountStore } from '@/stores/accountStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { useUiStore } from '@/stores/uiStore';
import { useComposerStore } from '@/features/composer/composerStore';
import { documentToEditor } from '@/features/composer/documentWire';
import type { MessageRowView } from '@/services/surfaces';

/** A mailbox-list row, overridable per test. */
function row(over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key: { message: { account: 'work', key: 'imap:v1:u1' } },
    account_id: 'work',
    sender_name: 'Ada Lovelace',
    sender_address: 'ada@example.test',
    subject: 'Launch',
    sent_at: '2026-10-09T08:30:00Z',
    unread: true,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

function resetStores(): void {
  useUiStore.getState().setShellView('mail');
  useAccountStore.setState({ accounts: [{ accountId: 'work', displayName: 'Work' }] });
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useListStore.setState({ rows: [], selectedKey: null });
  useComposerStore.setState({
    open: false,
    resumeAttempted: false,
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
    sending: false,
    sendError: null,
  });
}

/** Seeds an open reading message. */
function openMessage(): void {
  useReadingStore.setState({
    snapshot: {
      message: { account: 'work', key: 'imap:v1:u1' },
      subject: 'Launch',
      from_name: 'Ada Lovelace',
      from_address: 'ada@example.test',
      sent_at: '2026-10-09T08:30:00Z',
      plain: 'The launch is ready.',
      remote_image_count: 0,
    },
  });
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  resetStores();
});

afterEach(cleanup);

describe('the ribbon shell', () => {
  it('stands in the mail view and hides over calendar and people', () => {
    render(<CommandRibbon />);
    expect(screen.getByRole('navigation', { name: 'Command ribbon' })).toBeTruthy();

    act(() => {
      useUiStore.getState().setShellView('calendar');
    });
    expect(screen.queryByRole('navigation', { name: 'Command ribbon' })).toBeNull();

    act(() => {
      useUiStore.getState().setShellView('people');
    });
    expect(screen.queryByRole('navigation', { name: 'Command ribbon' })).toBeNull();
  });

  it('collapses to icon-only buttons under the compact breakpoint', () => {
    const width = Object.getOwnPropertyDescriptor(window, 'innerWidth');
    Object.defineProperty(window, 'innerWidth', { value: 800, configurable: true });
    try {
      render(<CommandRibbon />);
      const button = screen.getByRole('button', { name: 'New Email' });
      expect(button.querySelector('span.sr-only')?.textContent).toBe('New Email');
    } finally {
      if (width !== undefined) {
        Object.defineProperty(window, 'innerWidth', width);
      }
    }
  });
});

describe('the read mode', () => {
  it('opens a plain composition from New Email, as the floating action does', () => {
    render(<CommandRibbon />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'New Email' }));
    });

    const state = useComposerStore.getState();
    expect(state.open).toBe(true);
    expect(state.composition).not.toBeNull();
    expect(state.from).toBe('work');
  });

  it('keeps New Email disabled without an account', () => {
    useAccountStore.setState({ accounts: [] });
    render(<CommandRibbon />);

    const button = screen.getByRole('button', { name: 'New Email' }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.title).toBe('Add an account first');
  });

  it('dispatches each compose intent with the open message exactly', () => {
    openMessage();
    render(<CommandRibbon />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Reply' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"compose":{"reply":{"message":{"account":"work","key":"imap:v1:u1"}}}}',
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Reply all' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"compose":{"reply_all":{"message":{"account":"work","key":"imap:v1:u1"}}}}',
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Forward' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[2]?.[0])).toBe(
      '{"compose":{"forward":{"message":{"account":"work","key":"imap:v1:u1"}}}}',
    );
  });

  it('stands the compose verbs disabled with nothing open', () => {
    render(<CommandRibbon />);

    for (const name of ['Reply', 'Reply all', 'Forward']) {
      const button = screen.getByRole('button', { name }) as HTMLButtonElement;
      expect(button.disabled).toBe(true);
    }

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Reply' }));
    });
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('marks the selected row read and flags it, labels naming the state they leave', () => {
    useListStore.setState({ rows: [row()], selectedKey: row().key });
    render(<CommandRibbon />);

    // The row is unread and unflagged: the verbs offer Mark read and Flag.
    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Mark read' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"mark_read":{"row":{"message":{"account":"work","key":"imap:v1:u1"}},"read":true}}',
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Flag' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"set_flagged":{"row":{"message":{"account":"work","key":"imap:v1:u1"}},"flagged":true}}',
    );
  });

  it('offers Mark unread and Unflag on a read, flagged row', () => {
    const settled = row({ unread: false, flagged: true });
    useListStore.setState({ rows: [settled], selectedKey: settled.key });
    render(<CommandRibbon />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Mark unread' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"mark_read":{"row":{"message":{"account":"work","key":"imap:v1:u1"}},"read":false}}',
    );

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Unflag' }));
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"set_flagged":{"row":{"message":{"account":"work","key":"imap:v1:u1"}},"flagged":false}}',
    );
  });

  it('stands the row verbs disabled without a selection', () => {
    useListStore.setState({ rows: [row()], selectedKey: null });
    render(<CommandRibbon />);

    for (const name of ['Mark unread', 'Flag']) {
      const button = screen.getByRole('button', { name }) as HTMLButtonElement;
      expect(button.disabled).toBe(true);
    }

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Flag' }));
    });
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });
});

describe('the compose mode', () => {
  function openComposer(): void {
    act(() => {
      useComposerStore.getState().newMessage({ id: 'work', label: 'Work' });
    });
  }

  it('switches modes with the composer session', () => {
    render(<CommandRibbon />);
    expect(screen.getByRole('button', { name: 'New Email' })).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Send' })).toBeNull();

    openComposer();

    expect(screen.queryByRole('button', { name: 'New Email' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Send' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Snippets' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Discard' })).toBeTruthy();
  });

  it('gates Send on a recipient, then submits through the store path', async () => {
    openComposer();
    render(<CommandRibbon />);

    const send = screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);

    act(() => {
      const store = useComposerStore.getState();
      store.setField('to', 'you@test.local');
      store.setField('subject', 'Launch');
    });
    expect(send.disabled).toBe(false);

    act(() => {
      fireEvent.click(send);
    });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toContain('"submit_rich_mail"');
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toContain('"to":"you@test.local"');
    // An accepted send clears the stored draft and closes the composition.
    await act(async () => {
      await Promise.resolve();
    });
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toContain('"discard"');
    expect(useComposerStore.getState().open).toBe(false);
  });

  it('discards the stored copy and closes the composition from Discard', () => {
    openComposer();
    render(<CommandRibbon />);
    const composition = useComposerStore.getState().composition as string;

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      `{"drafts":{"discard":{"composition":"${composition}"}}}`,
    );
    expect(useComposerStore.getState().open).toBe(false);
  });
});
