// SPDX-License-Identifier: GPL-3.0-only
// The smart replies strip: the open message's chips render with an accept
// affordance each, the accept hands the chip's whole drafted reply to the
// compose-from-draft host command, the dismissal records its write and
// clears the strip, and the strip stands absent with no chips to show. The
// pull itself rides the ai chips store, pinned in its own battery.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ai = vi.hoisted(() => ({
  chipsFor: vi.fn(),
  dismissChips: vi.fn(),
  composeFromDraft: vi.fn(),
}));

vi.mock('@/services/ai', () => ai);

import { SmartReplies } from './SmartReplies';
import { useAiChipsStore } from '@/stores/aiChipsStore';
import { useReadingStore } from '@/stores/readingStore';
import type { ReadingSnapshot } from '@/services/ipc';

/** The open message's snapshot, the pane's own fixture shape. */
function snapshot(): ReadingSnapshot {
  return {
    message: { account: 'work', key: 'imap:v1:u1' },
    subject: 'Quarterly report',
    from_name: 'Ada Lovelace',
    from_address: 'ada@example.test',
    sent_at: '2026-10-09T08:30:00Z',
    remote_image_count: 0,
  };
}

beforeEach(() => {
  ai.chipsFor.mockReset();
  ai.dismissChips.mockReset();
  ai.composeFromDraft.mockReset();
  ai.chipsFor.mockResolvedValue([]);
  ai.dismissChips.mockResolvedValue(undefined);
  ai.composeFromDraft.mockResolvedValue(undefined);
  useAiChipsStore.setState({ message: null, chips: [] });
});

afterEach(cleanup);

describe('the smart replies strip', () => {
  it('renders the pulled chips, each with its preview and an accept affordance', async () => {
    ai.chipsFor.mockResolvedValue([
      { reply: 'Sounds good, shipped it.', preview: 'Sounds good, shipped it.' },
      { reply: 'I will look tomorrow.', preview: 'I will look tomorrow.' },
    ]);
    useReadingStore.getState().applySnapshot(snapshot());
    render(<SmartReplies />);

    await waitFor(() => {
      expect(screen.getByText('Sounds good, shipped it.')).toBeTruthy();
    });
    expect(screen.getByText('I will look tomorrow.')).toBeTruthy();
    expect(
      screen.getByRole('button', { name: 'Reply with: Sounds good, shipped it.' }),
    ).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Reply with: I will look tomorrow.' })).toBeTruthy();
  });

  it('accepts a chip by handing its whole drafted reply to the compose command', async () => {
    ai.chipsFor.mockResolvedValue([{ reply: 'Shipped.', preview: 'Shipped.' }]);
    useReadingStore.getState().applySnapshot(snapshot());
    render(<SmartReplies />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Reply with: Shipped.' })).toBeTruthy();
    });
    fireEvent.click(screen.getByRole('button', { name: 'Reply with: Shipped.' }));

    expect(ai.composeFromDraft).toHaveBeenCalledWith(
      { account: 'work', key: 'imap:v1:u1' },
      'Shipped.',
    );
  });

  it('dismisses by recording the write and clearing the strip', async () => {
    ai.chipsFor.mockResolvedValue([{ reply: 'Shipped.', preview: 'Shipped.' }]);
    useReadingStore.getState().applySnapshot(snapshot());
    render(<SmartReplies />);

    await waitFor(() => {
      expect(screen.getByText('Shipped.')).toBeTruthy();
    });
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss smart replies' }));

    expect(ai.dismissChips).toHaveBeenCalledWith({ account: 'work', key: 'imap:v1:u1' });
    expect(screen.queryByText('Shipped.')).toBeNull();
  });

  it('stands absent with no chips to show and with nothing open', async () => {
    useReadingStore.getState().applySnapshot(null);
    const { container } = render(<SmartReplies />);

    await waitFor(() => {
      expect(ai.chipsFor).not.toHaveBeenCalled();
    });
    expect(container.firstChild).toBeNull();
  });
});
