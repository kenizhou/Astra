// SPDX-License-Identifier: GPL-3.0-only
// The month grid: the fixed six-week civil grid around the anchor month,
// chips bucketed per zone-local day, the all-day banners chipped once on
// the day they start so the overflow count names occurrences rather than
// repeats, the overflow affordance into the day view, and today's cell
// named from the snapshot's now data.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { MonthGrid } from './MonthGrid';
import { useCalendarStore } from './calendarStore';
import type { OccurrenceRow } from '@/services/ipc';

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

const pickDay = vi.fn();

beforeEach(() => {
  pickDay.mockReset();
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
    view: 'month',
    anchor: '2026-10-15',
  });
});

afterEach(cleanup);

describe('the month grid', () => {
  it('renders the six weeks of the anchor month with dimmed neighbours', () => {
    render(<MonthGrid onPickDay={pickDay} onSelectEvent={() => undefined} />);

    // October 2026 opens on a Thursday: the grid runs 28 September to
    // 8 November, and the neighbouring months render as their own kind.
    const first = screen.getByLabelText('Monday 28 September 2026');
    const today = screen.getByLabelText('Monday 19 October 2026');
    const last = screen.getByLabelText('Sunday 8 November 2026');
    expect(first.getAttribute('data-in-month')).toBe('false');
    expect(today.getAttribute('data-in-month')).toBe('true');
    expect(today.getAttribute('data-today')).toBe('true');
    expect(last.getAttribute('data-in-month')).toBe('false');
  });

  it('buckets the day chips by their zone-local day', () => {
    useCalendarStore.setState({
      occurrences: [
        row(),
        row({ event: 'b', title: 'Late shift', start: '2026-10-19T21:00:00Z', end: '2026-10-19T22:00:00Z' }),
        // 20 October at 00:30 Amsterdam time: 22:30Z on the 19th.
        row({ event: 'c', title: 'Past midnight', start: '2026-10-19T22:30:00Z', end: '2026-10-19T23:00:00Z' }),
      ],
    });
    render(<MonthGrid onPickDay={pickDay} onSelectEvent={() => undefined} />);

    const nineteenth = screen.getByLabelText('Monday 19 October 2026');
    const twentieth = screen.getByLabelText('Tuesday 20 October 2026');
    expect(nineteenth.textContent).toContain('Standup');
    expect(nineteenth.textContent).toContain('Late shift');
    expect(twentieth.textContent).toContain('Past midnight');
  });

  it('caps the chips at three and offers the rest through the day view', () => {
    useCalendarStore.setState({
      occurrences: [1, 2, 3, 4, 5].map((n) =>
        row({ event: `e${n}`, title: `Event ${n}`, start: '2026-10-20T07:00:00Z', end: '2026-10-20T08:00:00Z' }),
      ),
    });
    render(<MonthGrid onPickDay={pickDay} onSelectEvent={() => undefined} />);

    const twentieth = screen.getByLabelText('Tuesday 20 October 2026');
    expect(twentieth.textContent).toContain('Event 1');
    expect(twentieth.textContent).not.toContain('Event 4');
    fireEvent.click(screen.getByRole('button', { name: '2 more on Tuesday 20 October 2026' }));

    expect(pickDay).toHaveBeenCalledWith('2026-10-20');
  });

  it('marks a recurring chip with its repeat glyph and a hold distinctly', () => {
    useCalendarStore.setState({
      occurrences: [
        row({ event: 'r', title: 'Weekly', recurring: true }),
        row({ event: 'h', title: 'Unanswered', participation: 'needs_action' }),
      ],
    });
    render(<MonthGrid onPickDay={pickDay} onSelectEvent={() => undefined} />);

    expect(screen.getByTitle('Weekly repeats')).toBeTruthy();
    const hold = screen.getByText('Unanswered').closest('[data-chip]');
    expect(hold!.getAttribute('data-hold')).toBe('true');
  });

  it('answers a click on a day header with that day', () => {
    render(<MonthGrid onPickDay={pickDay} onSelectEvent={() => undefined} />);

    fireEvent.click(screen.getByRole('button', { name: 'Go to Thursday 22 October 2026' }));

    expect(pickDay).toHaveBeenCalledWith('2026-10-22');
  });
});

describe('the month grid banners', () => {
  it('shows a spanning all-day occurrence once, on the day it starts', () => {
    useCalendarStore.setState({
      occurrences: [
        row({
          event: 'conf',
          title: 'Conference',
          all_day: true,
          accuracy: 'all_day',
          start: '2026-10-18T22:00:00Z',
          end: '2026-10-21T22:00:00Z',
        }),
      ],
    });
    render(<MonthGrid onPickDay={pickDay} onSelectEvent={() => undefined} />);

    const nineteenth = screen.getByLabelText('Monday 19 October 2026');
    const twentieth = screen.getByLabelText('Tuesday 20 October 2026');
    const twentyFirst = screen.getByLabelText('Wednesday 21 October 2026');
    expect(nineteenth.textContent).toContain('Conference');
    expect(twentieth.textContent).not.toContain('Conference');
    expect(twentyFirst.textContent).not.toContain('Conference');
    // The chip itself says the occurrence runs on: a banner, not a day.
    const banner = nineteenth.querySelector('[data-span]');
    expect(banner).not.toBeNull();
    expect(banner!.getAttribute('data-span')).toBe('true');
  });

  it('counts the overflow over banners and timed chips alike', () => {
    useCalendarStore.setState({
      occurrences: [
        // The banner opens on the 19th and runs to the 20th: midnight in
        // Amsterdam is 22:00Z the evening before.
        row({
          event: 'conf',
          title: 'Conference',
          all_day: true,
          accuracy: 'all_day',
          start: '2026-10-18T22:00:00Z',
          end: '2026-10-20T22:00:00Z',
        }),
        ...[1, 2, 3].map((n) =>
          row({
            event: `t${n}`,
            title: `Timed ${n}`,
            start: '2026-10-20T07:00:00Z',
            end: '2026-10-20T08:00:00Z',
          }),
        ),
      ],
    });
    render(<MonthGrid onPickDay={pickDay} onSelectEvent={() => undefined} />);

    // The banner started on the 19th, so the 20th holds the three timed
    // chips alone: every one shows, no overflow counts the banner again.
    const twentieth = screen.getByLabelText('Tuesday 20 October 2026');
    expect(twentieth.textContent).toContain('Timed 1');
    expect(twentieth.textContent).toContain('Timed 3');
    expect(twentieth.textContent).not.toContain('more on');

    // A fourth timed chip pushes the count over: one more, the banner
    // nowhere in the arithmetic.
    act(() => {
      useCalendarStore.setState({
        occurrences: [1, 2, 3, 4].map((n) =>
          row({
            event: `t${n}`,
            title: `Timed ${n}`,
            start: '2026-10-22T07:00:00Z',
            end: '2026-10-22T08:00:00Z',
          }),
        ),
      });
    });
    expect(
      screen.getByRole('button', { name: '1 more on Thursday 22 October 2026' }),
    ).toBeTruthy();
  });
});
