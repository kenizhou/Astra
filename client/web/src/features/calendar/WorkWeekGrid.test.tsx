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

const settingsState = vi.hoisted(() => ({
  settings: { first_day_of_week: 'mo' },
}));

vi.mock('@/features/settings/settingsStore', () => ({
  useSettingsStore: (selector: (state: unknown) => unknown) => selector(settingsState),
}));

import { WorkWeekGrid } from './WorkWeekGrid';
import { useCalendarStore } from './calendarStore';
import type { OccurrenceRow } from '@/services/ipc';

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
    render(<WorkWeekGrid onSelectEvent={() => undefined} />);

    screen.getByLabelText('Work week grid, 19 October to 23 October 2026');
    expect(screen.getByText('Mon')).toBeTruthy();
    expect(screen.getByText('Fri')).toBeTruthy();
    expect(screen.queryByText('Sat')).toBeNull();
  });

  it('honours the first-day setting in the week layout', () => {
    settingsState.settings.first_day_of_week = 'su';
    render(<WorkWeekGrid onSelectEvent={() => undefined} />);

    screen.getByLabelText('Work week grid, 18 October to 22 October 2026');
    expect(screen.getByText('Sun')).toBeTruthy();
    expect(screen.queryByText('Sat')).toBeNull();
    settingsState.settings.first_day_of_week = 'mo';
  });

  it('places each row in the column of its own day', () => {
    render(<WorkWeekGrid onSelectEvent={() => undefined} />);

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
    render(<WorkWeekGrid onSelectEvent={() => undefined} />);

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

describe('the work-week grid all-day lane', () => {
  /** An all-day row over the given Amsterdam days, as UTC midnights. */
  function allDay(over: Record<string, unknown> = {}): OccurrenceRow {
    return {
      account: 'work',
      event: '/dav/cal/work/default/conf.ics',
      calendar: 'cal-1',
      title: 'Conference',
      start: '2026-10-19T22:00:00Z',
      end: '2026-10-21T22:00:00Z',
      all_day: true,
      recurring: false,
      overridden: false,
      accuracy: 'all_day',
      participation: 'accepted',
      can_move: true,
      series_edit_warns: false,
      ...over,
    } as OccurrenceRow;
  }

  it('joins a multi-day all-day occurrence into one spanning banner', () => {
    useCalendarStore.setState({
      occurrences: [allDay({ start: '2026-10-19T22:00:00Z', end: '2026-10-21T22:00:00Z' })],
    });
    render(<WorkWeekGrid onSelectEvent={() => undefined} />);

    const banners = document.querySelectorAll('[data-banner]');
    expect(banners).toHaveLength(1);
    // Tuesday and Wednesday Amsterdam: the banner joins both columns.
    expect(banners[0]!.getAttribute('style')).toContain('grid-column: 2 / span 2');
    expect(banners[0]!.textContent).toContain('Conference');
    expect(banners[0]!.getAttribute('data-continues-before')).toBe('false');
    expect(banners[0]!.getAttribute('data-continues-after')).toBe('false');
  });

  it('marks the continuation into days the window hides', () => {
    // From the Saturday before the week to the Saturday after it.
    useCalendarStore.setState({
      occurrences: [allDay({ start: '2026-10-16T22:00:00Z', end: '2026-10-24T22:00:00Z' })],
    });
    render(<WorkWeekGrid onSelectEvent={() => undefined} />);

    const banner = document.querySelector('[data-banner]')!;
    expect(banner.getAttribute('style')).toContain('grid-column: 1 / span 5');
    expect(banner.getAttribute('data-continues-before')).toBe('true');
    expect(banner.getAttribute('data-continues-after')).toBe('true');
    // The clip marks read: the banner says it runs on past both edges.
    expect(banner.textContent).toContain('‹');
    expect(banner.textContent).toContain('›');
  });

  it('stacks two banners that share days in their own lanes', () => {
    useCalendarStore.setState({
      occurrences: [
        allDay({
          event: 'a',
          title: 'First',
          start: '2026-10-19T22:00:00Z',
          end: '2026-10-21T22:00:00Z',
        }),
        allDay({
          event: 'b',
          title: 'Second',
          start: '2026-10-20T22:00:00Z',
          end: '2026-10-22T22:00:00Z',
        }),
      ],
    });
    render(<WorkWeekGrid onSelectEvent={() => undefined} />);

    const banners = document.querySelectorAll('[data-banner]');
    expect(banners).toHaveLength(2);
    expect(banners[0]!.textContent).toContain('First');
    expect(banners[0]!.getAttribute('style')).toContain('grid-row: 1');
    expect(banners[1]!.textContent).toContain('Second');
    expect(banners[1]!.getAttribute('style')).toContain('grid-row: 2');
  });
});
