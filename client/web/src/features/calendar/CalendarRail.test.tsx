// SPDX-License-Identifier: GPL-3.0-only
// The calendar rail: the mini-month navigator moves the navigation anchor
// (and the core's window with it) without leaving the active view, its
// chevrons browse months without touching the anchor, and the visibility
// checkboxes dispatch the same visible-set intent the toolbar's manager
// sends.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

const settingsState = vi.hoisted(() => ({
  settings: { first_day_of_week: 'mo', twenty_four_clock: true },
}));

vi.mock('@/features/settings/settingsStore', () => ({
  useSettingsStore: (selector: (state: unknown) => unknown) => selector(settingsState),
}));

import { CalendarRail } from './CalendarRail';
import { useCalendarStore } from './calendarStore';

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
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
        visible: false,
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
    selectedEvent: null,
  });
});

afterEach(cleanup);

describe('the mini-month navigator', () => {
  it('moves the anchor on a day click and loads the window, the standing view riding it', () => {
    render(<CalendarRail />);

    fireEvent.click(screen.getByRole('button', { name: 'Go to 2026-10-20' }));

    expect(useCalendarStore.getState().anchor).toBe('2026-10-20');
    expect(useCalendarStore.getState().view).toBe('month');
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"set_date":{"date":"2026-10-20"}}}',
    );
    expect(JSON.stringify(ipc.dispatch.mock.calls[1]?.[0])).toBe(
      '{"calendar":{"set_range":{"start":"2026-09-27T22:00:00Z","days":42,"view":"month"}}}',
    );
  });

  it('marks the anchored day and today', () => {
    render(<CalendarRail />);

    expect(
      screen.getByRole('button', { name: 'Go to 2026-10-15' }).getAttribute('aria-pressed'),
    ).toBe('true');
    expect(
      screen.getByRole('button', { name: 'Go to 2026-10-19' }).className,
    ).toContain('bg-primary');
  });

  it('browses months with the chevrons without moving the anchor', () => {
    render(<CalendarRail />);
    expect(screen.getByText('October 2026')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Next month' }));

    expect(screen.getByText('November 2026')).toBeTruthy();
    expect(useCalendarStore.getState().anchor).toBe('2026-10-15');
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('follows the anchor when navigation lands in another month', () => {
    render(<CalendarRail />);

    act(() => {
      useCalendarStore.getState().setAnchor('2026-12-24');
    });

    expect(screen.getByText('December 2026')).toBeTruthy();
  });
});

describe('the visibility list', () => {
  it('hides a calendar, naming the visible set without it', () => {
    render(<CalendarRail />);

    fireEvent.click(screen.getByRole('checkbox', { name: 'Show Work' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"set_visible_calendars":{"ids":[]}}}',
    );
  });

  it('shows a hidden calendar, adding it to the visible set', () => {
    render(<CalendarRail />);

    fireEvent.click(screen.getByRole('checkbox', { name: 'Show Personal' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"set_visible_calendars":{"ids":[{"account":"work","key":"cal-1"},' +
        '{"account":"work","key":"cal-2"}]}}}',
    );
  });
});
