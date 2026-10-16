// SPDX-License-Identifier: GPL-3.0-only
// The reading pane: the empty state with nothing open, the full pane from a
// snapshot (header, gated body, attachment rows), the remote toggle through
// the reading store, the plain fallback, the no-body note, and the close
// intent crossing back. The compose actions live on the command ribbon now,
// pinned in its own suite. The AI replies surfaces (the chips strip and the
// summarise block) mount below the header.
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
  translateMessage: vi.fn(),
}));

vi.mock('@/services/ai', () => ai);

// The composer is stubbed: the takeover tests assert pane ownership, not
// the composer's internals (those live in QuickReplyStrip.test.tsx).
vi.mock('@/features/composer/Composer', () => ({
  ComposerPane: () => <div data-testid="composer-stub" />,
}));

import { ReadingPane } from './ReadingPane';
import { useAiChipsStore } from '@/stores/aiChipsStore';
import { useComposerStore } from '@/features/composer/composerStore';
import { usePeopleStore } from '@/features/people/peopleStore';
import { useReadingStore } from '@/stores/readingStore';
import { useUiStore } from '@/stores/uiStore';
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
  ai.translateMessage.mockReset();
  useReadingStore.setState({ snapshot: null, showRemoteImages: false });
  useAiChipsStore.setState({ message: null, chips: [] });
  useComposerStore.setState({ open: false, composition: null });
  usePeopleStore.setState({ pendingContactCreate: null });
  useUiStore.setState({ shellView: 'mail' });
});

afterEach(cleanup);

describe('the reading pane', () => {
  it('shows the empty state with nothing open', () => {
    render(<ReadingPane />);

    expect(screen.getByText('Select a message to read it.')).toBeTruthy();
    expect(screen.queryByTitle('Message body')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Close reading pane' })).toBeNull();
  });

  it('renders the header, the gated body, and the attachments from the snapshot', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    render(<ReadingPane />);

    expect(screen.getByText('Launch')).toBeTruthy();
    expect(screen.getByText('Ada Lovelace')).toBeTruthy();
    expect(screen.getByText('ada@example.test')).toBeTruthy();
    // The default clock setting is the core's own default: 12-hour.
    expect(screen.getByText('9 Oct 2026, 8:30 AM')).toBeTruthy();
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

  it('the inline composer owns the whole pane while a composition stands', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    useComposerStore.setState({ open: true, composition: { id: 'c1' } as never });
    render(<ReadingPane />);

    // kylins' InlineReply idiom: no header, no body, no strip while the
    // dock owns the pane.
    expect(screen.getByTestId('composer-stub')).toBeTruthy();
    expect(screen.queryByText('Launch')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Reply all' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'More actions' })).toBeNull();
  });

  it('hands the pane back to the message when the composition leaves', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    useComposerStore.setState({ open: false, composition: null });
    render(<ReadingPane />);

    expect(screen.getByText('Launch')).toBeTruthy();
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
    expect(screen.getByRole('button', { name: 'Translate' })).toBeTruthy();
    expect(screen.getByRole('combobox', { name: 'Translate to' })).toBeTruthy();
  });

  it('mounts the invitation card above the body when the snapshot carries one', () => {
    useReadingStore.getState().applySnapshot(
      snapshot({
        invitation: {
          kind: 'rsvp',
          response: 'needs_action',
          stale: false,
          summary: 'Sprint planning',
          organizer: 'boss@example.test',
        },
      }),
    );
    render(<ReadingPane />);

    const region = screen.getByRole('region', { name: 'Invitation' });
    expect(region.textContent).toContain('Sprint planning');
    expect(screen.getByRole('button', { name: 'Accept' })).toBeTruthy();
  });

  it('mounts no invitation card for a plain message', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    render(<ReadingPane />);

    expect(screen.queryByRole('region', { name: 'Invitation' })).toBeNull();
  });

  it('hands the sender to the people view from the add-contact chip', () => {
    useReadingStore.getState().applySnapshot(snapshot());
    render(<ReadingPane />);

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'Add to contacts' }));
    });

    expect(usePeopleStore.getState().pendingContactCreate).toEqual({
      name: 'Ada Lovelace',
      address: 'ada@example.test',
    });
    expect(useUiStore.getState().shellView).toBe('people');
  });

  it('offers no add-contact chip for a senderless message', () => {
    useReadingStore.getState().applySnapshot(snapshot({ from_name: '', from_address: '' }));
    render(<ReadingPane />);

    expect(screen.queryByRole('button', { name: 'Add to contacts' })).toBeNull();
  });
});
