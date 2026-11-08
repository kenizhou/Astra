// SPDX-License-Identifier: GPL-3.0-only
// The day grid: rows placed by their zone-local clock, the all-day lane, the
// now line drawn from the snapshot's now data and advanced by the host clock
// between publishes, holds drawn distinct from commitments, and the drag
// gestures: a drag from a block crosses as the move intent, a drag over
// empty time previews and claims a create slot, and a press that never
// leaves its slot is a click.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

const settingsState = vi.hoisted(() => ({
  settings: { twenty_four_clock: true },
}));

vi.mock('@/features/settings/settingsStore', () => ({
  useSettingsStore: (selector: (state: unknown) => unknown) => selector(settingsState),
}));

import { DayGrid } from './DayGrid';
import { useCalendarStore } from './calendarStore';
import type { OccurrenceRow } from '@/services/ipc';
import type { CreateSlot } from './useEventDrag';

/** A timed occurrence row at 09:00 zone time on 19 October 2026. */
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

function seedStore(occurrences: OccurrenceRow[]): void {
  useCalendarStore.setState({
    occurrences,
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
    view: 'day',
    anchor: '2026-10-19',
  });
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe('the day grid', () => {
  it('places a row by its zone-local clock', () => {
    seedStore([row()]);
    render(<DayGrid onSelectEvent={() => undefined} />);

    const block = screen.getByText('Standup').closest('[data-block]');
    // 09:00 to 09:30 zone time: 540 minutes of 44px hours, half an hour tall.
    expect(block).not.toBeNull();
    expect(block!.getAttribute('style')).toContain('top: 396px');
    expect(block!.getAttribute('style')).toContain('height: 22px');
  });

  it('renders the hour gutter in 12 hour form per the clock setting', () => {
    settingsState.settings.twenty_four_clock = false;
    seedStore([]);
    render(<DayGrid onSelectEvent={() => undefined} />);

    expect(screen.getByText('1 PM')).toBeTruthy();
    expect(screen.queryByText('13:00')).toBeNull();
    settingsState.settings.twenty_four_clock = true;
  });

  it('rides an all-day row in its own lane above the grid', () => {
    seedStore([row({ all_day: true, start: '2026-10-18T22:00:00Z', end: '2026-10-19T22:00:00Z', accuracy: 'all_day' })]);
    render(<DayGrid onSelectEvent={() => undefined} />);

    const lane = screen.getByLabelText('All day lane');
    expect(lane.textContent).toContain('Standup');
    expect(screen.getByRole('button', { name: 'All day, Standup' })).toBeTruthy();
  });

  it('marks an unanswered invitation as a hold, distinct from a commitment', () => {
    seedStore([
      row({ event: 'hold', title: 'Hold me', participation: 'needs_action' }),
      row({ event: 'kept', title: 'Committed' }),
    ]);
    render(<DayGrid onSelectEvent={() => undefined} />);

    const hold = screen.getByText('Hold me').closest('[data-block]');
    const kept = screen.getByText('Committed').closest('[data-block]');
    expect(hold!.getAttribute('data-hold')).toBe('true');
    expect(kept!.getAttribute('data-hold')).toBe('false');
  });

  it('draws the now line from the snapshot zone and advances it on the host clock', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-10-19T08:12:00Z'));
    try {
      seedStore([row()]);
      render(<DayGrid onSelectEvent={() => undefined} />);

      // 10:12 in Amsterdam: 612 minutes down the 44px hours.
      let line = screen.getByLabelText('Now');
      expect(line.getAttribute('style')).toContain('top: 448.8px');

      // Twenty-seven minutes pass with no publish: the host clock alone
      // moves the line to 10:39.
      act(() => {
        vi.setSystemTime(new Date('2026-10-19T08:39:00Z'));
        vi.advanceTimersByTime(30_000);
      });
      line = screen.getByLabelText('Now');
      expect(line.getAttribute('style')).toContain('top: 468.6px');
    } finally {
      vi.useRealTimers();
    }
  });

  it('anchors the now line on the snapshot instant, not the host clock', () => {
    vi.useFakeTimers();
    // The host reads an hour past the snapshot's own instant.
    vi.setSystemTime(new Date('2026-10-19T09:12:00Z'));
    try {
      seedStore([row()]);
      render(<DayGrid onSelectEvent={() => undefined} />);

      // The line stands where the snapshot placed it (10:12 in Amsterdam)
      // and the skew does not displace it; only elapsed host time moves it.
      const line = screen.getByLabelText('Now');
      expect(line.getAttribute('style')).toContain('top: 448.8px');
    } finally {
      vi.useRealTimers();
    }
  });

  it('hides the now line on a day that is not today', () => {
    seedStore([row()]);
    useCalendarStore.getState().setAnchor('2026-10-20');

    render(<DayGrid onSelectEvent={() => undefined} />);

    expect(screen.queryByLabelText('Now')).toBeNull();
  });

  it('turns a pointer drag into the move intent with the occurrence token', () => {
    seedStore([
      row({
        recurring: true,
        occurrence_start: '2026-10-19T09:00:00',
        event: '/dav/cal/work/default/standup.ics',
      }),
    ]);
    render(<DayGrid onSelectEvent={() => undefined} />);

    const block = screen.getByText('Standup').closest('[data-block]')!;
    const grid = screen.getByLabelText('Day grid, Monday 19 October 2026');
    // Grab the block's top edge and drop it a slot down.
    fireEvent.pointerDown(block, { pointerId: 1, clientX: 60, clientY: 396 });
    fireEvent.pointerMove(grid, { pointerId: 1, clientX: 60, clientY: 407 });
    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 60, clientY: 407 });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/standup.ics"},' +
        '"drag":{"edge":"whole","days":0,"minutes":15,"occurrence":"2026-10-19T09:00:00"}}}}',
    );
  });

  it('moves a whole series when the row carries no occurrence token', () => {
    seedStore([row({ event: '/dav/cal/work/default/one.ics' })]);
    render(<DayGrid onSelectEvent={() => undefined} />);

    const block = screen.getByText('Standup').closest('[data-block]')!;
    const grid = screen.getByLabelText('Day grid, Monday 19 October 2026');
    fireEvent.pointerDown(block, { pointerId: 1, clientX: 60, clientY: 396 });
    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 60, clientY: 385 });

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/one.ics"},' +
        '"drag":{"edge":"whole","days":0,"minutes":-15}}}}',
    );
  });

  it('holds the dragged position optimistically until the next publish', () => {
    seedStore([row()]);
    render(<DayGrid onSelectEvent={() => undefined} />);

    const grid = screen.getByLabelText('Day grid, Monday 19 October 2026');
    fireEvent.pointerDown(screen.getByText('Standup').closest('[data-block]')!, {
      pointerId: 1,
      clientX: 60,
      clientY: 396,
    });
    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 60, clientY: 407 });

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    // No publish has confirmed the write yet: the block stands a slot down,
    // at 09:15, rather than snapping back to the row the snapshot still holds.
    expect(screen.getByText('Standup').closest('[data-block]')!.getAttribute('style')).toContain(
      'top: 407px',
    );

    // The next publish lands without the write behind it: the optimistic
    // position goes and the block renders the snapshot's own row again.
    act(() => {
      useCalendarStore.getState().applySnapshot({
        calendars: [],
        occurrences: [row()],
        now_data: { now: '2026-10-19T08:12:00Z', zone: 'Europe/Amsterdam' },
      });
    });
    expect(screen.getByText('Standup').closest('[data-block]')!.getAttribute('style')).toContain(
      'top: 396px',
    );
  });

  it('opens the event from the keyboard on Enter and Space', () => {
    const theRow = row();
    seedStore([theRow]);
    const opened: OccurrenceRow[] = [];
    render(<DayGrid onSelectEvent={(r) => opened.push(r)} />);

    const block = screen.getByText('Standup').closest('[data-block]')!;
    fireEvent.keyDown(block, { key: 'Enter' });
    fireEvent.keyDown(block, { key: ' ' });

    expect(opened).toEqual([theRow, theRow]);
  });

  it('never drags a block the calendar says is not movable', () => {
    seedStore([row({ event: 'no-move', title: 'Theirs', can_move: false })]);
    render(<DayGrid onSelectEvent={() => undefined} />);

    const block = screen.getByText('Theirs').closest('[data-block]')!;
    const grid = screen.getByLabelText('Day grid, Monday 19 October 2026');
    fireEvent.pointerDown(block, { pointerId: 1, clientX: 60, clientY: 396 });
    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 60, clientY: 407 });

    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('asks for a prefilled create from a drag over empty time', () => {
    seedStore([]);
    const slots: CreateSlot[] = [];
    render(
      <DayGrid onSelectEvent={() => undefined} onCreateSlot={(slot) => slots.push(slot)} />,
    );

    const grid = screen.getByLabelText('Day grid, Monday 19 October 2026');
    // Down at 09:00, out at 10:00: the slot the editor opens prefilled with.
    fireEvent.pointerDown(grid, { pointerId: 1, clientX: 60, clientY: 396 });
    fireEvent.pointerMove(grid, { pointerId: 1, clientX: 60, clientY: 440 });
    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 60, clientY: 440 });

    expect(slots).toEqual([{ day: '2026-10-19', startMinutes: 540, endMinutes: 600 }]);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('claims at least the anchor hour on the shortest real drag', () => {
    seedStore([]);
    const slots: CreateSlot[] = [];
    render(
      <DayGrid onSelectEvent={() => undefined} onCreateSlot={(slot) => slots.push(slot)} />,
    );

    const grid = screen.getByLabelText('Day grid, Monday 19 October 2026');
    // Down at 09:00, up one slot later: the slot still spans the whole hour.
    fireEvent.pointerDown(grid, { pointerId: 1, clientX: 60, clientY: 396 });
    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 60, clientY: 407 });

    expect(slots).toEqual([{ day: '2026-10-19', startMinutes: 540, endMinutes: 600 }]);
  });

  it('creates nothing from a press that never leaves its slot', () => {
    seedStore([]);
    const slots: CreateSlot[] = [];
    render(
      <DayGrid onSelectEvent={() => undefined} onCreateSlot={(slot) => slots.push(slot)} />,
    );

    const grid = screen.getByLabelText('Day grid, Monday 19 October 2026');
    fireEvent.pointerDown(grid, { pointerId: 1, clientX: 60, clientY: 396 });
    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 60, clientY: 396 });

    expect(slots).toEqual([]);
  });

  it('draws the preview band while a create drag runs', () => {
    seedStore([]);
    render(<DayGrid onSelectEvent={() => undefined} onCreateSlot={() => undefined} />);

    const grid = screen.getByLabelText('Day grid, Monday 19 October 2026');
    fireEvent.pointerDown(grid, { pointerId: 1, clientX: 60, clientY: 396 });
    fireEvent.pointerMove(grid, { pointerId: 1, clientX: 60, clientY: 407 });

    const ghost = document.querySelector('[data-create-ghost]');
    expect(ghost).not.toBeNull();
    expect(ghost!.getAttribute('style')).toContain('top: 396px');
    expect(ghost!.getAttribute('style')).toContain('height: 44px');

    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 60, clientY: 407 });
    expect(document.querySelector('[data-create-ghost]')).toBeNull();
  });
});
