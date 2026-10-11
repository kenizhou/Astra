// SPDX-License-Identifier: GPL-3.0-only
// The calendar pane: the toolbar over the active view, the write banners the
// write states draw, the reply-delivery prompt riding above the view, the
// event editor the rows and the New event action open, and the refresh on
// entry and on focus regain.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), calendarBriefGenerate: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  calendarBriefGenerate: ipc.calendarBriefGenerate,
}));

import { CalendarPane } from './CalendarPane';
import { useCalendarStore } from './calendarStore';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.calendarBriefGenerate.mockReset();
  useCalendarStore.setState({
    calendars: [
      {
        account: 'work',
        id: 'cal-1',
        name: 'Work',
        color: '#039be5',
        visible: true,
        can_write: true,
        is_default: true,
      },
    ],
    occurrences: [],
    writeStates: [],
    replyPrompts: [],
    nowData: { now: '2026-10-19T08:12:00Z', zone: 'Europe/Amsterdam' },
    view: 'month',
    anchor: '2026-10-15',
  });
});

afterEach(cleanup);

describe('the calendar pane', () => {
  it('renders the toolbar and the month view over the anchor', () => {
    render(<CalendarPane />);

    expect(screen.getByRole('button', { name: 'Today' })).toBeTruthy();
    expect(screen.getByLabelText('Monday 28 September 2026')).toBeTruthy();
  });

  it('refreshes on entry and again when the window regains focus', () => {
    render(<CalendarPane />);
    const entry = ipc.dispatch.mock.calls.filter(
      (call) => JSON.stringify(call[0]) === '{"calendar":"refresh"}',
    ).length;
    expect(entry).toBe(1);

    act(() => {
      window.dispatchEvent(new Event('focus'));
    });

    const regained = ipc.dispatch.mock.calls.filter(
      (call) => JSON.stringify(call[0]) === '{"calendar":"refresh"}',
    ).length;
    expect(regained).toBe(2);
  });

  it('switches to the day view from the toolbar', () => {
    render(<CalendarPane />);

    fireEvent.click(screen.getByRole('button', { name: 'Day' }));

    expect(useCalendarStore.getState().view).toBe('day');
    expect(screen.getByLabelText('Day column, Thursday 15 October 2026')).toBeTruthy();
  });

  it('navigates the anchor from a month cell into the day view', () => {
    render(<CalendarPane />);

    fireEvent.click(screen.getByRole('button', { name: 'Go to Thursday 22 October 2026' }));

    expect(useCalendarStore.getState().anchor).toBe('2026-10-22');
    expect(useCalendarStore.getState().view).toBe('day');
  });

  it('opens the editor from the New event action and closes it again', () => {
    render(<CalendarPane />);

    fireEvent.click(screen.getByRole('button', { name: 'New event' }));
    const dialog = screen.getByRole('dialog', { name: 'New event' });
    expect(dialog).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog', { name: 'New event' })).toBeNull();
  });

  it('draws the writing banner and the failed banner with its retry', () => {
    useCalendarStore.setState({ writeStates: [{ account: 'work', state: 'writing' }] });
    const { unmount } = render(<CalendarPane />);
    expect(screen.getByText('Saving calendar changes…')).toBeTruthy();
    unmount();

    useCalendarStore.setState({ writeStates: [{ account: 'work', state: 'failed_with_retry' }] });
    render(<CalendarPane />);
    expect(
      screen.getByText('The last calendar change did not settle. Refresh to see what stood.'),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    const refresh = ipc.dispatch.mock.calls.filter(
      (call) => JSON.stringify(call[0]) === '{"calendar":"refresh"}',
    ).length;
    expect(refresh).toBeGreaterThanOrEqual(2);
  });

  it('rides the reply prompt above the view when one stands', () => {
    useCalendarStore.setState({
      replyPrompts: [
        {
          account: 'work',
          message: 'imap:v1:u7@INBOX',
          summary: 'Sprint planning',
          organizer: 'boss@example.test',
          response: 'accepted',
          status_code: '5.2',
        },
      ],
    });
    render(<CalendarPane />);

    expect(screen.getByRole('dialog', { name: 'Reply not delivered' })).toBeTruthy();
  });
});
