// SPDX-License-Identifier: GPL-3.0-only
// The agenda view: the snapshot's occurrences grouped by zone-local day and
// laid out chronologically, each row carrying its clock under the clock
// setting, its calendar's colour, its repeat marker, and its hold marking;
// the empty state when the window holds nothing.
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { AgendaView } from './AgendaView';
import { useCalendarStore } from './calendarStore';
import { useSettingsStore } from '@/features/settings/settingsStore';
import type { OccurrenceRow, SettingsSnapshot } from '@/services/ipc';

/** A timed occurrence row, defaulting onto 19 October 2026. */
function row(over: Partial<OccurrenceRow> = {}): OccurrenceRow {
  return {
    account: 'work',
    event: '/dav/cal/work/default/evt-1.ics',
    calendar: 'cal-1',
    title: 'Standup',
    start: '2026-10-19T07:00:00Z',
    end: '2026-10-19T07:30:00Z',
    all_day: false,
    recurring: false,
    overridden: false,
    accuracy: 'zoned',
    participation: 'accepted',
    can_move: true,
    series_edit_warns: false,
    ...over,
  };
}

const open = vi.fn();

/** A settings snapshot over the clock word the caller names. */
function clockSnapshot(twentyFour: boolean): SettingsSnapshot {
  return {
    settings: {
      theme: 'follow_host',
      twenty_four_clock: twentyFour,
      zone: 'Etc/UTC',
      first_day_of_week: 'mo',
      horizon_days: 42,
      quote_below: true,
      view_mode: 'flat',
    },
    accounts: [],
    about: { version: '0.1.0' },
  };
}

beforeEach(() => {
  open.mockReset();
  useCalendarStore.setState({
    occurrences: [],
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
    view: 'agenda',
    anchor: '2026-10-19',
  });
});

afterEach(cleanup);

describe('the agenda view', () => {
  it('groups the rows under their zone-local day headers', () => {
    useCalendarStore.setState({
      occurrences: [row(), row({ event: 'b', title: 'Next day', start: '2026-10-20T07:00:00Z', end: '2026-10-20T08:00:00Z' })],
    });
    render(<AgendaView onSelectEvent={open} />);

    expect(screen.getByText('Monday 19 October')).toBeTruthy();
    expect(screen.getByText('Tuesday 20 October')).toBeTruthy();
    // 07:00Z is nine in the morning in Amsterdam; the default clock is
    // 12-hour, so both rows read their meridiem form.
    expect(screen.getAllByText('9 AM')).toHaveLength(2);
  });

  it('answers the clock setting the settings block carries', () => {
    useCalendarStore.setState({ occurrences: [row()] });
    useSettingsStore.getState().applySnapshot(clockSnapshot(true));
    render(<AgendaView onSelectEvent={open} />);

    expect(screen.getByText('09:00')).toBeTruthy();
    useSettingsStore.getState().applySnapshot(clockSnapshot(false));
  });

  it('sorts the rows within a day by their start', () => {
    useCalendarStore.setState({
      occurrences: [
        row({ event: 'b', title: 'Later', start: '2026-10-19T09:00:00Z', end: '2026-10-19T10:00:00Z' }),
        row({ event: 'a', title: 'Earlier', start: '2026-10-19T07:00:00Z', end: '2026-10-19T08:00:00Z' }),
      ],
    });
    render(<AgendaView onSelectEvent={open} />);

    const rows = screen.getAllByRole('listitem');
    expect(rows.map((node) => node.textContent)).toEqual([
      expect.stringContaining('Earlier'),
      expect.stringContaining('Later'),
    ]);
  });

  it('shows an all-day row as all day, and the hold and repeat markers', () => {
    useCalendarStore.setState({
      occurrences: [
        row({ event: 'd', title: 'Away day', all_day: true, start: '2026-10-19T07:00:00Z', end: '2026-10-19T08:00:00Z', accuracy: 'all_day' }),
        row({ event: 'h', title: 'Unanswered', participation: 'needs_action' }),
        row({ event: 'r', title: 'Weekly', recurring: true }),
      ],
    });
    render(<AgendaView onSelectEvent={open} />);

    expect(screen.getByText('All day')).toBeTruthy();
    expect(screen.getByTitle('Unanswered awaits your answer')).toBeTruthy();
    expect(screen.getByTitle('Weekly repeats')).toBeTruthy();
  });

  it('draws the empty state when the window holds nothing', () => {
    render(<AgendaView onSelectEvent={open} />);

    expect(screen.getByText('No events in this window.')).toBeTruthy();
  });

  it('opens the event on a row click', () => {
    const target = row();
    useCalendarStore.setState({ occurrences: [target] });
    render(<AgendaView onSelectEvent={open} />);

    screen.getByText('Standup').click();

    expect(open).toHaveBeenCalledWith(target);
  });
});
