// SPDX-License-Identifier: GPL-3.0-only
// The row's hover quick actions: archive and trash beside the flag and
// read/unread buttons in the trailing slot, revealed on hover, selection,
// or focus within the row, each dispatching the hovered row's own key with
// the exact intent payload, a button click never selecting or opening the
// row, the mail four suppressed on draft rows, and a flagged row keeping
// its flag visible without hover.
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { MessageRow } from './MessageRow';
import { useAppearanceStore } from '@/services/appearance';
import { useFolderStore } from '@/stores/folderStore';
import type { MessageRowView, RowKey } from '@/services/surfaces';

function row(key: RowKey, over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key,
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'Quarterly report',
    sent_at: '2026-10-09T08:30:00Z',
    unread: true,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

const messageKey = { message: { account: 'work', key: 'k1' } };
const threadKey = { thread: { account: 'work', thread_id: 'th-1' } };

interface DrawOptions {
  selected?: boolean;
  multiSelected?: boolean;
  draft?: boolean;
  onSelect?: (key: RowKey, event: React.MouseEvent) => void;
}

/** Renders the row directly, the swipe test's pattern without the toast:
 *  hover actions own no undo. */
function draw(target: MessageRowView, opts: DrawOptions = {}): void {
  render(
    <MessageRow
      row={target}
      selected={opts.selected ?? false}
      multiSelected={opts.multiSelected ?? false}
      draft={opts.draft ?? false}
      now={new Date('2026-10-10T00:00:00Z')}
      onSelect={opts.onSelect ?? vi.fn()}
    />,
  );
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useAppearanceStore.setState({
    swipePrimary: 'archive',
    swipeSecondary: 'trash',
  } as Partial<ReturnType<typeof useAppearanceStore.getState>>);
  useFolderStore.setState({ accounts: [], selected: { account: 'work', key: 'INBOX' }, outboxAccount: null, unified: false });
});

afterEach(cleanup);

describe('the hover quick actions', () => {
  it('reveals archive and trash on hover and hides them on leave', () => {
    draw(row(messageKey));
    const option = screen.getByRole('option');

    expect(within(option).queryByRole('button', { name: 'Archive' })).toBeNull();
    expect(within(option).queryByRole('button', { name: 'Delete' })).toBeNull();

    fireEvent.mouseEnter(option);

    expect(within(option).getByRole('button', { name: 'Archive' })).toBeTruthy();
    expect(within(option).getByRole('button', { name: 'Delete' })).toBeTruthy();

    fireEvent.mouseLeave(option);

    expect(within(option).queryByRole('button', { name: 'Archive' })).toBeNull();
    expect(within(option).queryByRole('button', { name: 'Delete' })).toBeNull();
  });

  it('reveals the slot on a selected row without hover', () => {
    draw(row(messageKey), { selected: true });
    const option = screen.getByRole('option');

    expect(within(option).getByRole('button', { name: 'Archive' })).toBeTruthy();
    expect(within(option).getByRole('button', { name: 'Delete' })).toBeTruthy();
  });

  it('keeps the slot revealed while focus stays within the row', () => {
    draw(row(messageKey));
    const option = screen.getByRole('option');
    fireEvent.mouseEnter(option);
    const archive = within(option).getByRole('button', { name: 'Archive' });
    fireEvent.focus(archive);

    // The pointer leaves, the focus stays: the slot holds.
    fireEvent.mouseLeave(option);
    expect(within(option).getByRole('button', { name: 'Archive' })).toBeTruthy();
    expect(within(option).getByRole('button', { name: 'Delete' })).toBeTruthy();

    // Focus stepping between the row's own buttons keeps the reveal.
    const trash = within(option).getByRole('button', { name: 'Delete' });
    fireEvent.blur(archive, { relatedTarget: trash });
    fireEvent.focus(trash);
    expect(within(option).getByRole('button', { name: 'Archive' })).toBeTruthy();

    // Focus leaving the row outright hides the slot again.
    fireEvent.blur(trash, { relatedTarget: null });
    expect(within(option).queryByRole('button', { name: 'Archive' })).toBeNull();
    expect(within(option).queryByRole('button', { name: 'Delete' })).toBeNull();
  });

  it('keeps the row itself out of the tab order', () => {
    draw(row(messageKey), { selected: true });
    const option = screen.getByRole('option');

    expect(option.getAttribute('tabindex')).toBeNull();
    expect(within(option).getByRole('button', { name: 'Archive' }).tagName).toBe('BUTTON');
  });

  it('archives the hovered message row with the exact payload, never selecting it', () => {
    const onSelect = vi.fn();
    draw(row(messageKey), { onSelect });
    const option = screen.getByRole('option');
    fireEvent.mouseEnter(option);

    fireEvent.click(within(option).getByRole('button', { name: 'Archive' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      archive_message: { row: { message: { account: 'work', key: 'k1' } } },
    });
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('moves the hovered message row to trash with the exact payload, never selecting it', () => {
    const onSelect = vi.fn();
    draw(row(messageKey), { onSelect });
    const option = screen.getByRole('option');
    fireEvent.mouseEnter(option);

    fireEvent.click(within(option).getByRole('button', { name: 'Delete' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      move_to_trash: { row: { message: { account: 'work', key: 'k1' } } },
    });
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('archives a thread row on its thread key', () => {
    const onSelect = vi.fn();
    draw(row(threadKey), { onSelect });
    const option = screen.getByRole('option');
    fireEvent.mouseEnter(option);

    fireEvent.click(within(option).getByRole('button', { name: 'Archive' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      archive_message: { row: { thread: { account: 'work', thread_id: 'th-1' } } },
    });
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('moves a thread row to trash on its thread key', () => {
    const onSelect = vi.fn();
    draw(row(threadKey), { onSelect });
    const option = screen.getByRole('option');
    fireEvent.mouseEnter(option);

    fireEvent.click(within(option).getByRole('button', { name: 'Delete' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      move_to_trash: { row: { thread: { account: 'work', thread_id: 'th-1' } } },
    });
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('acts on the hovered row alone even when it rides a multi-selection', () => {
    const onSelect = vi.fn();
    draw(row(messageKey), { multiSelected: true, onSelect });
    const option = screen.getByRole('option');
    fireEvent.mouseEnter(option);

    fireEvent.click(within(option).getByRole('button', { name: 'Archive' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      archive_message: { row: { message: { account: 'work', key: 'k1' } } },
    });
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('suppresses the mail actions on a draft row', () => {
    draw(row(messageKey), { draft: true });
    const option = screen.getByRole('option');
    fireEvent.mouseEnter(option);

    expect(within(option).queryByRole('button', { name: 'Archive' })).toBeNull();
    expect(within(option).queryByRole('button', { name: 'Delete' })).toBeNull();
    expect(within(option).queryByRole('button', { name: 'Flag' })).toBeNull();
    expect(within(option).queryByRole('button', { name: 'Mark as read' })).toBeNull();
    // The Draft chip stays: the suppression is the mail four alone.
    expect(within(option).getByText('Draft')).toBeTruthy();
  });

  it('suppresses the flag on a flagged draft row too', () => {
    draw(row(messageKey, { flagged: true }), { draft: true });
    const option = screen.getByRole('option');

    expect(within(option).queryByRole('button', { name: 'Unflag' })).toBeNull();
  });

  it('keeps a flagged row flag visible without hover', () => {
    draw(row(messageKey, { flagged: true }));
    const option = screen.getByRole('option');

    const unflag = within(option).getByRole('button', { name: 'Unflag' });
    expect(unflag.getAttribute('aria-pressed')).toBe('true');
    expect(within(option).queryByRole('button', { name: 'Archive' })).toBeNull();

    fireEvent.mouseEnter(option);
    expect(within(option).getByRole('button', { name: 'Archive' })).toBeTruthy();

    fireEvent.mouseLeave(option);
    expect(within(option).getByRole('button', { name: 'Unflag' })).toBeTruthy();
    expect(within(option).queryByRole('button', { name: 'Archive' })).toBeNull();
  });

  it('still reveals flag and read/unread on hover', () => {
    draw(row(messageKey));
    const option = screen.getByRole('option');

    fireEvent.mouseEnter(option);
    expect(within(option).getByRole('button', { name: 'Flag' })).toBeTruthy();
    expect(within(option).getByRole('button', { name: 'Mark as read' })).toBeTruthy();

    fireEvent.mouseLeave(option);
    expect(within(option).queryByRole('button', { name: 'Flag' })).toBeNull();
    expect(within(option).queryByRole('button', { name: 'Mark as read' })).toBeNull();
  });
});
