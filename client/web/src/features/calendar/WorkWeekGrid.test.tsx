// SPDX-License-Identifier: GPL-3.0-only
// The work-week grid: five zone-local weekday columns over the shared time
// grid, each row in the column of its own day, and a drag across columns
// crossing as the day delta the move intent takes.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { WorkWeekGrid } from './WorkWeekGrid';
import { useCalendarStore } from './calendarStore';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useCalendarStore.setState({
    occurrences: [
      {
        account: 'work',
        event: '/dav/cal/work/default/evt-1.ics',
        calendar: 'cal-1',
        title: 'Monday standup',
        start: '2026-10-19T07:00:00Z',
        end: '2026-10-19T07:30:00Z',
        all_day: false,
        recurring: false,
        overridden: false,
        accuracy: 'zoned',
        participation: 'accepted',
        can_move: true,
        series_edit_warns: false,
      },
      {
        account: 'work',
        event: '/dav/cal/work/default/evt-2.ics',
        calendar: 'cal-1',
        title: 'Wednesday review',
        start: '2026-10-21T07:00:00Z',
        end: '2026-10-21T08:00:00Z',
        all_day: false,
        recurring: false,
        overridden: false,
        accuracy: 'zoned',
        participation: 'needs_action',
        can_move: true,
        series_edit_warns: false,
      },
    ],
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
    nowData: { now: '2026-10-19T08:12:00Z', zone: 'Europe/Amsterdam' },
    view: 'work_week',
    anchor: '2026-10-21',
  });
});

afterEach(cleanup);

describe('the work-week grid', () => {
  it('lays out the five weekdays of the anchor week', () => {
    render(<WorkWeekGrid onOpenEvent={() => undefined} />);

    screen.getByLabelText('Work week grid, 19 October to 23 October 2026');
    expect(screen.getByText('Mon')).toBeTruthy();
    expect(screen.getByText('Fri')).toBeTruthy();
    expect(screen.queryByText('Sat')).toBeNull();
  });

  it('places each row in the column of its own day', () => {
    render(<WorkWeekGrid onOpenEvent={() => undefined} />);

    const monday = screen.getByLabelText('Day column, Monday 19 October 2026');
    const wednesday = screen.getByLabelText('Day column, Wednesday 21 October 2026');
    expect(monday.textContent).toContain('Monday standup');
    expect(monday.textContent).not.toContain('Wednesday review');
    expect(wednesday.textContent).toContain('Wednesday review');
    // The Wednesday hold draws dotted.
    const hold = screen.getByText('Wednesday review').closest('[data-block]');
    expect(hold!.getAttribute('data-hold')).toBe('true');
  });

  it('crosses a drag between columns as the day delta', () => {
    render(<WorkWeekGrid onOpenEvent={() => undefined} />);

    const block = screen.getByText('Monday standup').closest('[data-block]')!;
    const grid = screen.getByLabelText('Work week grid, 19 October to 23 October 2026');
    // Down on the Monday column at 09:00, up on the Tuesday column's 09:30.
    fireEvent.pointerDown(block, { pointerId: 1, clientX: 60, clientY: 396 });
    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 160, clientY: 407 });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"drag":{"edge":"whole","days":1,"minutes":15}}}}',
    );
  });
});
