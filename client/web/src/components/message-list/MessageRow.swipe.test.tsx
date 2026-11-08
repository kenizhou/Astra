// SPDX-License-Identifier: GPL-3.0-only
// The row's touch swipe: a horizontal touch pointer gesture reveals the
// direction's action behind the row and fires it past the threshold, the
// undo toast reverses the move, and the gesture never owns the other two
// channels: the mouse pointer (the folder drag's) never starts a swipe, a
// vertical-dominant touch move hands the axis back to the virtualiser's
// scroll, and a swiped row does not also open on the click that follows.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { MessageRow } from './MessageRow';
import { SwipeToast } from './SwipeToast';
import { useSwipeToastStore } from './swipeToastStore';
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

/** Renders the row with the toast mounted, the way the list pane draws them. */
function draw(target: MessageRowView, onSelect = vi.fn()): void {
  render(
    <>
      <MessageRow
        row={target}
        selected={false}
        now={new Date('2026-10-10T00:00:00Z')}
        onSelect={onSelect}
      />
      <SwipeToast />
    </>,
  );
}

/** One touch pointer gesture from startX to endX on the row. */
function touchSwipe(startX: number, endX: number, startY = 10, endY = startY): void {
  const target = screen.getByRole('option');
  fireEvent.pointerDown(target, { pointerType: 'touch', pointerId: 7, clientX: startX, clientY: startY, button: 0 });
  fireEvent.pointerMove(target, { pointerType: 'touch', pointerId: 7, clientX: endX, clientY: endY, buttons: 1 });
  fireEvent.pointerUp(target, { pointerType: 'touch', pointerId: 7, clientX: endX, clientY: endY });
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useAppearanceStore.setState({
    swipePrimary: 'archive',
    swipeSecondary: 'trash',
  } as Partial<ReturnType<typeof useAppearanceStore.getState>>);
  useFolderStore.setState({ accounts: [], selected: { account: 'work', key: 'INBOX' }, outboxAccount: null, unified: false });
  useSwipeToastStore.getState().dismiss();
});

afterEach(cleanup);

describe('the touch swipe', () => {
  it('reveals the primary action behind a right swipe and fires it past the threshold', () => {
    draw(row(messageKey));

    const target = screen.getByRole('option');
    fireEvent.pointerDown(target, { pointerType: 'touch', pointerId: 7, clientX: 50, clientY: 10, button: 0 });
    fireEvent.pointerMove(target, { pointerType: 'touch', pointerId: 7, clientX: 120, clientY: 10, buttons: 1 });

    expect(screen.getByTestId('swipe-reveal').textContent).toContain('Archive');

    fireEvent.pointerUp(target, { pointerType: 'touch', pointerId: 7, clientX: 150, clientY: 10 });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"archive_message":{"row":{"message":{"account":"work","key":"k1"}}}}',
    );
    expect(screen.getByRole('status').textContent).toContain('Archived');
  });

  it('fires the secondary action on a left swipe', () => {
    draw(row(messageKey));

    touchSwipe(150, 20);

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"move_to_trash":{"row":{"message":{"account":"work","key":"k1"}}}}',
    );
    expect(screen.getByRole('status').textContent).toContain('Moved to Trash');
  });

  it('undoes the move by filing the message back into the folder it sat in', () => {
    draw(row(messageKey));
    touchSwipe(150, 20);

    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"move_messages":{"rows":[{"account":"work","key":"k1"}],"folder":{"account":"work","key":"INBOX"}}}',
    );
  });

  it('fires on a thread row but offers no undo a move batch cannot name', () => {
    draw(row({ thread: { account: 'work', thread_id: 'th-1' } }));
    touchSwipe(150, 20);

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"move_to_trash":{"row":{"thread":{"account":"work","thread_id":"th-1"}}}}',
    );
    expect(screen.queryByRole('button', { name: 'Undo' })).toBeNull();
  });

  it('never starts a swipe on the mouse pointer: the folder drag owns it', () => {
    draw(row(messageKey));

    const target = screen.getByRole('option');
    fireEvent.pointerDown(target, { pointerType: 'mouse', pointerId: 1, clientX: 50, clientY: 10, button: 0 });
    fireEvent.pointerMove(target, { pointerType: 'mouse', pointerId: 1, clientX: 200, clientY: 10, buttons: 1 });
    fireEvent.pointerUp(target, { pointerType: 'mouse', pointerId: 1, clientX: 200, clientY: 10 });

    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('hands a vertical-dominant touch move back to the virtualiser scroll', () => {
    draw(row(messageKey));

    touchSwipe(50, 60, 10, 240);

    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('snaps back under the threshold without firing', () => {
    draw(row(messageKey));

    touchSwipe(50, 90);

    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('keeps the vertical axis native so the virtualiser scrolls it: pan-y', () => {
    draw(row(messageKey));

    expect(screen.getByRole('option').style.touchAction).toBe('pan-y');
  });

  it('does not open the row on the click that follows a fired swipe', () => {
    const onSelect = vi.fn();
    draw(row(messageKey), onSelect);
    touchSwipe(150, 20);

    fireEvent.click(screen.getByRole('option'));

    expect(onSelect).not.toHaveBeenCalled();
  });
});
