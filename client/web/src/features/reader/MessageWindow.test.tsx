// SPDX-License-Identifier: GPL-3.0-only
// The pop-out reading window's root: it opens its message through the same
// open_message intent the pane's row click sends (so the mark-read rides the
// host's one guarded open path and lands once from either reader), pulls its
// own session (the reading snapshot, the settings, the account tree), and
// renders the whole reading view; the open reading moving off its message,
// or closing, closes the OS window.
import { StrictMode } from 'react';
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  mailboxList: vi.fn(),
  settings: vi.fn(),
  reading: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  mailboxList: ipc.mailboxList,
  settings: ipc.settings,
  reading: ipc.reading,
}));

vi.mock('@/events/surfaceEvents', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isTauri: true,
}));

const currentWindow = vi.hoisted(() => ({ close: vi.fn() }));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: currentWindow.close,
    minimize: vi.fn(async () => undefined),
    toggleMaximize: vi.fn(async () => undefined),
    isMaximized: vi.fn(async () => false),
    onResized: vi.fn(async () => () => undefined),
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => undefined),
}));

const ai = vi.hoisted(() => ({
  chipsFor: vi.fn(),
  dismissChips: vi.fn(),
  composeFromDraft: vi.fn(),
  summariseMessage: vi.fn(),
  translateMessage: vi.fn(),
}));

vi.mock('@/services/ai', () => ai);

vi.mock('@/features/composer/Composer', () => ({
  ComposerPane: () => <div data-testid="composer-stub" />,
}));

import type { ReadingSnapshot } from '@/services/ipc';
import { useFolderStore } from '@/stores/folderStore';
import { useReadingStore } from '@/stores/readingStore';

import { MessageWindow } from './MessageWindow';

const message = { account: 'work', key: 'imap:v1:u42' };

/** The settings pull's answer: the shipped defaults plus an about block. */
const settingsSnapshot = {
  settings: {
    theme: 'light',
    twenty_four_clock: false,
    zone: 'Etc/UTC',
    first_day_of_week: 'mo',
    horizon_days: 42,
    quote_below: true,
    view_mode: 'flat',
  },
  accounts: [],
  about: { version: 'test' },
};

/** The open message's reading snapshot. */
function readingSnapshot(over: Partial<ReadingSnapshot> = {}): ReadingSnapshot {
  return {
    message,
    subject: 'Launch',
    from_name: 'Ada Lovelace',
    from_address: 'ada@example.test',
    sent_at: '2026-10-09T08:30:00Z',
    html: '<p>The launch is ready.</p>',
    plain: 'The launch is ready.',
    attachments: [{ id: 1, file_name: 'report.pdf', media_type: 'application/pdf', size: 4096 }],
    remote_image_count: 0,
    ...over,
  };
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.mailboxList.mockReset();
  ipc.mailboxList.mockResolvedValue({
    account_folders: [
      {
        account_id: 'work',
        display_name: 'Work',
        address: 'me@work.example',
        expanded: true,
        can_report_mail: true,
        folders: [
          { key: 'junk', name: 'Junk', role: 'junk', unread: 0, depth: 0, has_children: false, expanded: false },
        ],
      },
    ],
  });
  ipc.settings.mockReset();
  ipc.settings.mockResolvedValue(settingsSnapshot);
  ipc.reading.mockReset();
  ipc.reading.mockResolvedValue(readingSnapshot());
  ai.chipsFor.mockReset();
  ai.chipsFor.mockResolvedValue([]);
  ai.summariseMessage.mockReset();
  ai.translateMessage.mockReset();
  currentWindow.close.mockReset();
  currentWindow.close.mockResolvedValue(undefined);
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useFolderStore.setState({ accounts: [], selected: null, outboxAccount: null, unified: false });
});

afterEach(cleanup);

/** Flushes the hydration's pull chain. */
async function settle(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe('the window hydration', () => {
  it('opens its message through the same open_message intent the pane sends', async () => {
    render(<MessageWindow message={message} folderKey="junk" />);

    await settle();

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"open_message":{"message":{"account":"work","key":"imap:v1:u42"}}}',
    );
  });

  it('renders the whole reading view over its own snapshot pull', async () => {
    render(<MessageWindow message={message} folderKey={null} />);

    await settle();

    // The subject stands twice by design: the frameless title bar centres it
    // and the reading header draws it at the reading scale.
    expect(screen.getAllByText('Launch')).toHaveLength(2);
    expect(screen.getByText('Ada Lovelace')).toBeTruthy();
    expect(screen.getByTitle('Message body').getAttribute('srcdoc')).toContain(
      '<p>The launch is ready.</p>',
    );
    expect(screen.getByText('report.pdf')).toBeTruthy();
    // The Reply affordance stands twice by design: the verb row's icon and
    // the quick-reply strip's docked one.
    expect(screen.getAllByRole('button', { name: 'Reply' })).toHaveLength(2);
  });

  it('carries the junk-folder context onto the view, offering Not spam', async () => {
    render(<MessageWindow message={message} folderKey="junk" />);

    await settle();

    expect(screen.getByRole('button', { name: 'Not spam' })).toBeTruthy();
  });

  it('hydrates once under a StrictMode double mount', async () => {
    render(
      <StrictMode>
        <MessageWindow message={message} folderKey={null} />
      </StrictMode>,
    );

    await settle();

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.reading).toHaveBeenCalledTimes(1);
  });

  it('shows the preparing placeholder while the pulls are in flight', () => {
    ipc.reading.mockReturnValue(new Promise(() => undefined));
    render(<MessageWindow message={message} folderKey={null} />);

    expect(screen.getByRole('status').textContent).toBe('Opening the message');
  });
});

describe('the window lifetime', () => {
  it('closes the OS window when its message closes', async () => {
    render(<MessageWindow message={message} folderKey={null} />);
    await settle();

    act(() => {
      useReadingStore.getState().applySnapshot(null);
    });

    expect(currentWindow.close).toHaveBeenCalledTimes(1);
  });

  it('closes the OS window when the open reading moves to another message', async () => {
    render(<MessageWindow message={message} folderKey={null} />);
    await settle();

    act(() => {
      useReadingStore
        .getState()
        .applySnapshot(readingSnapshot({ message: { account: 'work', key: 'other' } }));
    });

    expect(currentWindow.close).toHaveBeenCalledTimes(1);
  });

  it('stays open over a republish of its own message', async () => {
    render(<MessageWindow message={message} folderKey={null} />);
    await settle();

    act(() => {
      useReadingStore.getState().applySnapshot(readingSnapshot({ subject: 'Launch (updated)' }));
    });

    expect(currentWindow.close).not.toHaveBeenCalled();
    expect(screen.getAllByText('Launch (updated)')).toHaveLength(2);
  });
});
