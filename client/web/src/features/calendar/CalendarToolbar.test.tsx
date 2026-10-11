// SPDX-License-Identifier: GPL-3.0-only
// The calendar toolbar: the navigation verbs (today, the period arrows), the
// view switch, the New event and Briefs actions, and the calendar manager
// with its colour dots and visibility toggles. Every navigation dispatches
// the anchor date and the window the core must load.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { CalendarToolbar } from './CalendarToolbar';
import { useCalendarStore } from './calendarStore';

const onCreate = vi.fn();
const onBriefs = vi.fn();

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  onCreate.mockReset();
  onBriefs.mockReset();
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
      {
        account: 'work',
        id: 'cal-2',
        name: 'Personal',
        visible: true,
        can_write: false,
        is_default: false,
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

describe('the calendar toolbar navigation', () => {
  it('jumps to today with the zone-local date and the view window', () => {
    render(<CalendarToolbar onCreate={onCreate} onBriefs={onBriefs} />);

    fireEvent.click(screen.getByRole('button', { name: 'Today' }));

    expect(useCalendarStore.getState().anchor).toBe('2026-10-19');
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"set_date":{"date":"2026-10-19"}}}',
    );
    // The month window over today opens at the grid's first day (28
    // September), whose midnight in Amsterdam is still 27 September in UTC
    // while summer time holds.
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"calendar":{"set_range":{"start":"2026-09-27T22:00:00Z","days":42}}}',
    );
  });

  it('steps the period by the view, a month here', () => {
    render(<CalendarToolbar onCreate={onCreate} onBriefs={onBriefs} />);

    fireEvent.click(screen.getByRole('button', { name: 'Previous period' }));
    expect(useCalendarStore.getState().anchor).toBe('2026-09-15');

    fireEvent.click(screen.getByRole('button', { name: 'Next period' }));
    expect(useCalendarStore.getState().anchor).toBe('2026-10-15');
  });

  it('steps a day at a time in the day view', () => {
    useCalendarStore.getState().setView('day');
    render(<CalendarToolbar onCreate={onCreate} onBriefs={onBriefs} />);

    fireEvent.click(screen.getByRole('button', { name: 'Next period' }));

    expect(useCalendarStore.getState().anchor).toBe('2026-10-16');
  });
});

describe('the calendar toolbar actions', () => {
  it('switches the view and loads that view window', () => {
    render(<CalendarToolbar onCreate={onCreate} onBriefs={onBriefs} />);

    fireEvent.click(screen.getByRole('button', { name: 'Day' }));

    expect(useCalendarStore.getState().view).toBe('day');
    const range = ipc.dispatch.mock.calls.find(
      (call) => JSON.stringify(call[0]).includes('set_range'),
    )?.[0];
    expect(JSON.stringify(range)).toBe(
      '{"calendar":{"set_range":{"start":"2026-10-14T22:00:00Z","days":1}}}',
    );
  });

  it('opens the editor and the briefs panel from their actions', () => {
    render(<CalendarToolbar onCreate={onCreate} onBriefs={onBriefs} />);

    fireEvent.click(screen.getByRole('button', { name: 'New event' }));
    expect(onCreate).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: 'Briefs' }));
    expect(onBriefs).toHaveBeenCalledTimes(1);
  });
});

describe('the calendar manager', () => {
  it('lists every calendar with its colour dot and visibility toggle', () => {
    render(<CalendarToolbar onCreate={onCreate} onBriefs={onBriefs} />);

    fireEvent.click(screen.getByRole('button', { name: 'Calendars' }));

    const work = screen.getByLabelText('Show Work');
    expect(work.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByLabelText('Show Personal').getAttribute('aria-pressed')).toBe('true');
  });

  it('names the visible set without the calendar switched off', () => {
    render(<CalendarToolbar onCreate={onCreate} onBriefs={onBriefs} />);

    fireEvent.click(screen.getByRole('button', { name: 'Calendars' }));
    fireEvent.click(screen.getByLabelText('Show Personal'));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"set_visible_calendars":{"ids":[{"account":"work","key":"cal-1"}]}}}',
    );
  });
});
