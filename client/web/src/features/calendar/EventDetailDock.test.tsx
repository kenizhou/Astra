// SPDX-License-Identifier: GPL-3.0-only
// The event detail dock: the picked occurrence's title, when, calendar, and
// recurrence read-back in the mail-calendar order, the write affordances
// gated on the calendar's writability, the one-vs-all question on a series
// delete, and the close that clears the pick.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
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

import { EventDetailDock } from './EventDetailDock';
import { useCalendarStore } from './calendarStore';
import type { OccurrenceRow } from '@/services/ipc';

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

function seed(selected: OccurrenceRow | null, canWrite = true): void {
  useCalendarStore.setState({
    calendars: [
      {
        account: 'work',
        id: 'cal-1',
        name: 'Work',
        color: '#039be5',
        visible: true,
        can_write: canWrite,
        is_default: true,
      },
    ],
    nowData: { now: '2026-10-19T08:12:00Z', zone: 'Europe/Amsterdam' },
    selectedEvent: selected,
  });
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  settingsState.settings.twenty_four_clock = true;
});

afterEach(cleanup);

describe('the event detail dock', () => {
  it('renders nothing while nothing is picked', () => {
    seed(null);
    const { container } = render(<EventDetailDock onEdit={() => undefined} />);

    expect(container.innerHTML).toBe('');
  });

  it('shows the title, the when line, and the calendar of the pick', () => {
    seed(row());
    render(<EventDetailDock onEdit={() => undefined} />);

    const dock = screen.getByLabelText('Event detail');
    expect(dock.textContent).toContain('Standup');
    expect(dock.textContent).toContain('Monday 19 October 2026, 09:00 to 09:30');
    expect(dock.textContent).toContain('Europe/Amsterdam');
    expect(dock.textContent).toContain('Work');
  });

  it('reads the clock under the 12 hour setting', () => {
    settingsState.settings.twenty_four_clock = false;
    seed(row());
    render(<EventDetailDock onEdit={() => undefined} />);

    expect(screen.getByLabelText('Event detail').textContent).toContain('9 AM to 9:30 AM');
  });

  it('shows an all-day event as its inclusive day', () => {
    seed(
      row({
        all_day: true,
        accuracy: 'all_day',
        start: '2026-10-18T22:00:00Z',
        end: '2026-10-19T22:00:00Z',
      }),
    );
    render(<EventDetailDock onEdit={() => undefined} />);

    expect(screen.getByLabelText('Event detail').textContent).toContain(
      'Monday 19 October 2026',
    );
  });

  it('reads the recurrence back where the snapshot carries it', () => {
    seed(row({ recurring: true, occurrence_start: '2026-10-19T09:00:00' }));
    render(<EventDetailDock onEdit={() => undefined} />);

    expect(
      screen.getByLabelText('Event detail').textContent,
    ).toContain('Repeats. The series holds its own rule.');
  });

  it('gates the write affordances on the calendar being writable', () => {
    seed(row(), false);
    render(<EventDetailDock onEdit={() => undefined} />);

    expect(screen.queryByRole('button', { name: 'Edit' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Delete' })).toBeNull();
  });

  it('opens the editor from Edit', () => {
    const theRow = row();
    seed(theRow);
    const edited: OccurrenceRow[] = [];
    render(<EventDetailDock onEdit={(r) => edited.push(r)} />);

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    expect(edited).toEqual([theRow]);
  });

  it('deletes a plain event at once and clears the pick', () => {
    seed(row());
    render(<EventDetailDock onEdit={() => undefined} />);

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"delete_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"}}}}',
    );
    expect(useCalendarStore.getState().selectedEvent).toBeNull();
  });

  it('asks one occurrence or the whole series before deleting a series', () => {
    seed(row({ recurring: true, occurrence_start: '2026-10-19T09:00:00' }));
    render(<EventDetailDock onEdit={() => undefined} />);

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(ipc.dispatch).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Delete this event' }));
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"delete_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"occurrence":"2026-10-19T09:00:00"}}}',
    );
  });

  it('clears the pick from the close button', () => {
    seed(row());
    render(<EventDetailDock onEdit={() => undefined} />);

    fireEvent.click(screen.getByRole('button', { name: 'Close event detail' }));

    expect(useCalendarStore.getState().selectedEvent).toBeNull();
  });
});
