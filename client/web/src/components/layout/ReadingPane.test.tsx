// SPDX-License-Identifier: GPL-3.0-only
// The reading pane: the empty state with nothing open (the compose actions
// standing disabled beside it), the full pane from a snapshot (header, gated
// body, attachment rows, the compose actions dispatching for the open
// message), the remote toggle through the reading store, the plain fallback,
// the no-body note, and the close intent crossing back. The AI replies
// surfaces (the chips strip and the summarise block) mount below the header.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({
  dispatch: vi.fn(),
  mailboxList: vi.fn(),
  syncProgress: vi.fn(),
  addAccount: vi.fn(),
  imapTestConnection: vi.fn(),
}));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

const ai = vi.hoisted(() => ({
  chipsFor: vi.fn(),
  dismissChips: vi.fn(),
  composeFromDraft: vi.fn(),
  summariseMessage: vi.fn(),
}));

vi.mock('@/services/ai', () => ai);

import { ReadingPane } from './ReadingPane';
import { useAiChipsStore } from '@/stores/aiChipsStore';
import { useReadingStore } from '@/stores/readingStore';
import type { ReadingSnapshot } from '@/services/ipc';

/** The sanitised body the Rust gate emits: one inline data URI, one remote
 * src placeholder, one remote CSS placeholder. */
const sanitisedHtml =
  '<p>The launch is ready.</p>' +
  '<img src="data:image/png;base64,aGVsbG8=">' +
  '<img data-astra-src="https://tracker.example/p.png">' +
  '<style>body{background:url(data-astra-style-url:https://x.example/a.png)}</style>';

/** The rich message's snapshot, overridable per test. */
function snapshot(over: Partial<ReadingSnapshot> = {}): ReadingSnapshot {
  return {
    message: { account: 'work', key: 'imap:v1:u1' },
    subject: 'Launch',
    from_name: 'Ada Lovelace',
    from_address: 'ada@example.test',
    sent_at: '2026-10-09T08:30:00Z',
    html: sanitisedHtml,
    plain: 'The launch is ready.',
    attachments: [{ id: 1, file_name: 'report.pdf', media_type: 'application/pdf', size: 4096 }],
    remote_image_count: 2,
    ...over,
  };
}

function frameDoc(): string {
  return screen.getByTitle('Message body').getAttribute('srcdoc') ?? '';
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ai.chipsFor.mockReset();
  ai.chipsFor.mockResolvedValue([]);
  ai.dismissChips.mockReset();
  ai.composeFromDraft.mockReset();
  ai.summariseMessage.mockReset();
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useAiChipsStore.setState({ message: null, chips: [] });
});

afterEach(cleanup);

describe('the reading pane', () => {
  it('shows the empty state with nothing open and the actions disabled', () => {
    render(<ReadingPane />);

    expect(screen.getByText('Select a message to read it.')).toBeTruthy();
    expect(screen.queryByTitle('Message body')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Close reading pane' })).toBeNull();
    for (const name of ['Reply', 'Reply all', 'Forward']) {
      const button = screen.getByRole('button', { name }) as HTMLButtonElement;
      expect(button.disabled).toBe(true);
    }
  });

  it('dispatches the compose family from the actions for the open message', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    render(<ReadingPane />);

    for (const name of ['Reply', 'Reply all', 'Forward']) {
      const button = screen.getByRole('button', { name }) as HTMLButtonElement;
      expect(button.disabled).toBe(false);
    }

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Reply all' }));
    });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"compose":{"reply_all":{"message":{"account":"work","key":"imap:v1:u1"}}}}',
    );
  });

  it('renders the header, the gated body, and the attachments from the snapshot', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    render(<ReadingPane />);

    expect(screen.getByText('Launch')).toBeTruthy();
    expect(screen.getByText('Ada Lovelace <ada@example.test>')).toBeTruthy();
    expect(screen.getByText('9 Oct 2026, 08:30')).toBeTruthy();
    expect(frameDoc()).toBe(sanitisedHtml);
    expect(frameDoc()).not.toContain(' src="http');
    expect(screen.getByText('2 remote images blocked.')).toBeTruthy();
    expect(screen.getByText('report.pdf')).toBeTruthy();
    expect(screen.getByText('4.0 KB')).toBeTruthy();
  });

  it('shows the remote images through the store when the reader opts in', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    render(<ReadingPane />);

    expect(screen.getByTitle('Message body').getAttribute('csp')).toBe(
      "default-src 'none'; img-src data:; style-src 'unsafe-inline'",
    );

    fireEvent.click(screen.getByRole('button', { name: 'Show remote images' }));

    expect(useReadingStore.getState().showRemoteImages).toBe(true);
    expect(frameDoc()).toContain('src="https://tracker.example/p.png"');
    expect(frameDoc()).toContain('url(https://x.example/a.png)');
    expect(screen.queryByText('2 remote images blocked.')).toBeNull();
    // The csp widens with the same opt-in, so a csp-honouring runtime loads
    // what the rewrite restored.
    expect(screen.getByTitle('Message body').getAttribute('csp')).toBe(
      "default-src 'none'; img-src data: https:; style-src 'unsafe-inline' https:",
    );
  });

  it('keeps the toggle state over a republish of the same message', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    render(<ReadingPane />);
    fireEvent.click(screen.getByRole('button', { name: 'Show remote images' }));

    act(() => {
      useReadingStore.getState().applySnapshot(snapshot({ subject: 'Launch (updated)' }));
    });

    expect(frameDoc()).toContain('src="https://tracker.example/p.png"');
    expect(screen.getByText('Launch (updated)')).toBeTruthy();
  });

  it('falls back to the plain body when the message carries no html', () => {
    useReadingStore
      .getState()
      .applySnapshot(snapshot({ html: undefined, remote_image_count: 0 }));
    render(<ReadingPane />);

    expect(screen.getByText('The launch is ready.')).toBeTruthy();
    expect(screen.queryByTitle('Message body')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Show remote images' })).toBeNull();
  });

  it('notes a message with no text body at all', () => {
    useReadingStore
      .getState()
      .applySnapshot(snapshot({ html: undefined, plain: undefined, remote_image_count: 0 }));
    render(<ReadingPane />);

    expect(screen.getByText('This message has no readable body.')).toBeTruthy();
  });

  it('dispatches close_reading from the close button', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    render(<ReadingPane />);

    fireEvent.click(screen.getByRole('button', { name: 'Close reading pane' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith('close_reading');
  });

  it('mounts the smart replies strip and the summarise block below the header', async () => {
    ai.chipsFor.mockResolvedValue([{ reply: 'Shipped.', preview: 'Shipped.' }]);
    useReadingStore.getState().applySnapshot(snapshot());
    render(<ReadingPane />);

    await waitFor(() => {
      expect(screen.getByText('Shipped.')).toBeTruthy();
    });
    expect(screen.getByRole('button', { name: 'Summarise' })).toBeTruthy();
    expect(ai.chipsFor).toHaveBeenCalledWith({ account: 'work', key: 'imap:v1:u1' });
  });
});
