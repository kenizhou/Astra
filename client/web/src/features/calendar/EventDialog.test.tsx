// SPDX-License-Identifier: GPL-3.0-only
// The event dialog: creation with the repeat choices, the edit split (a
// content edit through the update intent, a time edit through the drag
// currency, whose delta the core applies in the event's own wall clock), the
// end-only edit as the end-edge drag, the one-vs-all choice on a recurring
// row, the series warning that names what a series edit would discard, and
// the read-only recurrence degradation.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

const settingsState = vi.hoisted(() => ({
  settings: { default_calendar: undefined as { account: string; key: string } | undefined },
}));

vi.mock('@/features/settings/settingsStore', () => ({
  useSettingsStore: (selector: (state: unknown) => unknown) => selector(settingsState),
}));

import { EventDialog } from './EventDialog';
import { useCalendarStore } from './calendarStore';
import type { OccurrenceRow } from '@/services/ipc';

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

const onClose = vi.fn();

function seed(): void {
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
    view: 'day',
    anchor: '2026-10-19',
  });
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  onClose.mockReset();
  settingsState.settings.default_calendar = undefined;
  seed();
});

afterEach(cleanup);

describe('the event dialog create flow', () => {
  it('preselects the default calendar the settings block pins', () => {
    useCalendarStore.setState({
      calendars: [
        { account: 'work', id: 'cal-1', name: 'Work', color: '#039be5', visible: true, can_write: true, is_default: true },
        { account: 'home', id: 'cal-2', name: 'Home', color: '#8e24aa', visible: true, can_write: true, is_default: false },
      ],
    });
    settingsState.settings.default_calendar = { account: 'home', key: 'cal-2' };
    render(<EventDialog row={null} onClose={onClose} />);

    expect((screen.getByLabelText('Calendar') as HTMLSelectElement).value).toBe('home:cal-2');
  });

  it('falls back to the diary own default when no pin stands', () => {
    render(<EventDialog row={null} onClose={onClose} />);

    expect((screen.getByLabelText('Calendar') as HTMLSelectElement).value).toBe('work:cal-1');
  });

  it('falls back to the first writable when the pin names a calendar that no longer exists', () => {
    useCalendarStore.setState({
      calendars: [
        { account: 'work', id: 'cal-1', name: 'Work', color: '#039be5', visible: true, can_write: true, is_default: false },
        { account: 'home', id: 'cal-2', name: 'Home', color: '#8e24aa', visible: true, can_write: true, is_default: false },
      ],
    });
    settingsState.settings.default_calendar = { account: 'gone', key: 'cal-x' };
    render(<EventDialog row={null} onClose={onClose} />);

    expect((screen.getByLabelText('Calendar') as HTMLSelectElement).value).toBe('work:cal-1');
  });

  it('creates an event over the anchor day in the snapshot zone', () => {
    render(<EventDialog row={null} onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: 'Dentist' } });
    fireEvent.change(screen.getByLabelText('Start'), { target: { value: '2026-10-19T09:00' } });
    fireEvent.change(screen.getByLabelText('End'), { target: { value: '2026-10-19T09:30' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"create_event":{"calendar":{"account":"work","key":"cal-1"},"summary":"Dentist",' +
        '"start":{"Zoned":{"local":"2026-10-19T09:00:00","zone":{"Iana":"Europe/Amsterdam"}}},' +
        '"end":{"Zoned":{"local":"2026-10-19T09:30:00","zone":{"Iana":"Europe/Amsterdam"}}}}}}',
    );
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('creates a weekly series with the end the editor states', () => {
    render(<EventDialog row={null} onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: 'Standup' } });
    fireEvent.change(screen.getByLabelText('Start'), { target: { value: '2026-10-19T09:00' } });
    fireEvent.change(screen.getByLabelText('End'), { target: { value: '2026-10-19T09:30' } });
    fireEvent.change(screen.getByLabelText('Repeats'), { target: { value: 'weekly' } });
    fireEvent.change(screen.getByLabelText('Ends after'), { target: { value: '5' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toContain(
      '"recurrence":{"frequency":"weekly","interval":1,"weekdays":[],"end":{"after_count":5}}',
    );
  });

  it('creates an all-day event with the exclusive date form', () => {
    render(<EventDialog row={null} onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: 'Away day' } });
    fireEvent.click(screen.getByLabelText('All day'));
    fireEvent.change(screen.getByLabelText('Date'), { target: { value: '2026-10-19' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toContain(
      '"start":{"Date":"2026-10-19"},"end":{"Date":"2026-10-20"}',
    );
  });

  it('keeps the location and notes when they carry words', () => {
    render(<EventDialog row={null} onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: 'Review' } });
    fireEvent.change(screen.getByLabelText('Location'), { target: { value: 'Room A' } });
    fireEvent.change(screen.getByLabelText('Notes'), { target: { value: 'Bring the numbers' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toContain('"location":"Room A"');
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toContain(
      '"description":"Bring the numbers"',
    );
  });

  it('disables Save with a note when no calendar is writable', () => {
    useCalendarStore.setState({
      calendars: [
        {
          account: 'work',
          id: 'cal-1',
          name: 'Work',
          color: '#039be5',
          visible: true,
          can_write: false,
          is_default: true,
        },
      ],
    });
    render(<EventDialog row={null} onClose={onClose} />);

    expect(screen.getByText('There is no calendar you can write to.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Save' })).toHaveProperty('disabled', true);
  });
});

describe('the event dialog edit flow', () => {
  it('sends a content edit through the update intent with no times', () => {
    render(<EventDialog row={row()} onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: 'Renamed' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"update_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"edit":{"title":"Renamed"}}}}',
    );
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('sends a time edit through the drag currency, whole and zoned safely', () => {
    render(<EventDialog row={row()} onClose={onClose} />);

    // The row shows 09:00 to 09:30 in Amsterdam; the edit moves both to 10:00.
    fireEvent.change(screen.getByLabelText('Start'), { target: { value: '2026-10-19T10:00' } });
    fireEvent.change(screen.getByLabelText('End'), { target: { value: '2026-10-19T10:30' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"drag":{"edge":"whole","days":0,"minutes":60}}}}',
    );
  });

  it('sends a content edit and a time edit as their own two intents', () => {
    render(<EventDialog row={row()} onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: 'Renamed' } });
    fireEvent.change(screen.getByLabelText('Start'), { target: { value: '2026-10-19T09:15' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    const sent = ipc.dispatch.mock.calls.map((call) => JSON.stringify(call[0]));
    expect(sent).toContain(
      '{"calendar":{"update_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},"edit":{"title":"Renamed"}}}}',
    );
    expect(sent).toContain(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},"drag":{"edge":"whole","days":0,"minutes":15}}}}',
    );
  });

  it('sends an end-only edit as an end-edge drag carrying the end delta', () => {
    render(<EventDialog row={row()} onClose={onClose} />);

    // The row shows 09:00 to 09:30; only the end moves, to 10:00: a resize,
    // the end's own delta on the end edge.
    fireEvent.change(screen.getByLabelText('End'), { target: { value: '2026-10-19T10:00' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"drag":{"edge":"end","days":0,"minutes":30}}}}',
    );
  });

  it('scopes an end-only edit to the occurrence it stands on', () => {
    render(
      <EventDialog row={row({ recurring: true, occurrence_start: '2026-10-19T09:00:00' })} onClose={onClose} />,
    );

    fireEvent.change(screen.getByLabelText('End'), { target: { value: '2026-10-19T10:00' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"drag":{"edge":"end","days":0,"minutes":30,"occurrence":"2026-10-19T09:00:00"}}}}',
    );
  });

  it('sends nothing when neither edge nor content changed', () => {
    render(<EventDialog row={row()} onClose={onClose} />);

    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('carries a content edit on the one occurrence under This event', () => {
    render(
      <EventDialog row={row({ recurring: true, occurrence_start: '2026-10-19T09:00:00' })} onClose={onClose} />,
    );

    expect(screen.getByLabelText('Summary')).toHaveProperty('disabled', false);
    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: 'Renamed' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"update_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"edit":{"title":"Renamed","occurrence":"2026-10-19T09:00:00"}}}}',
    );
  });

  it('honours clearing the summary as a title clear', () => {
    render(<EventDialog row={row()} onClose={onClose} />);

    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"update_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"edit":{"title":""}}}}',
    );
  });

  it('shows the recurrence as read-only on a repeating event', () => {
    render(<EventDialog row={row({ recurring: true })} onClose={onClose} />);

    expect(screen.getByText('Repeats. The series holds its own rule.')).toBeTruthy();
    expect(screen.queryByLabelText('Repeats')).toBeNull();
  });
});

describe('the one-vs-all choice', () => {
  it('offers the choice only when the row repeats and carries its token', () => {
    const { unmount } = render(
      <EventDialog row={row({ recurring: true, occurrence_start: '2026-10-19T09:00:00' })} onClose={onClose} />,
    );
    expect(screen.getByLabelText('This event')).toBeTruthy();
    expect(screen.getByLabelText('All events')).toBeTruthy();
    unmount();

    render(<EventDialog row={row()} onClose={onClose} />);
    expect(screen.queryByLabelText('This event')).toBeNull();
  });

  it('scopes the time edit to the one occurrence under This event', () => {
    render(
      <EventDialog row={row({ recurring: true, occurrence_start: '2026-10-19T09:00:00' })} onClose={onClose} />,
    );

    fireEvent.change(screen.getByLabelText('Start'), { target: { value: '2026-10-19T10:00' } });
    fireEvent.change(screen.getByLabelText('End'), { target: { value: '2026-10-19T10:30' } });
    fireEvent.click(screen.getByLabelText('This event'));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"drag":{"edge":"whole","days":0,"minutes":60,"occurrence":"2026-10-19T09:00:00"}}}}',
    );
  });

  it('offers the delete choice one-vs-all on a repeating row', () => {
    render(
      <EventDialog row={row({ recurring: true, occurrence_start: '2026-10-19T09:00:00' })} onClose={onClose} />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    fireEvent.click(screen.getByRole('button', { name: 'Delete this event' }));
    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"delete_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"occurrence":"2026-10-19T09:00:00"}}}',
    );
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('deletes a one-off without a choice', () => {
    render(<EventDialog row={row()} onClose={onClose} />);

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"calendar":{"delete_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"}}}}',
    );
  });
});

describe('the series warning', () => {
  it('names what a series edit would discard and gates the save on it', () => {
    render(
      <EventDialog row={row({ recurring: true, occurrence_start: '2026-10-19T09:00:00', series_edit_warns: true })} onClose={onClose} />,
    );

    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: 'Renamed' } });
    fireEvent.click(screen.getByLabelText('All events'));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    // The warning stands between the save and the dispatch.
    expect(
      screen.getByText('Editing every event discards the changes made to individual occurrences.'),
    ).toBeTruthy();
    expect(ipc.dispatch).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Edit every event' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('never warns on a clean series', () => {
    render(
      <EventDialog row={row({ recurring: true, occurrence_start: '2026-10-19T09:00:00' })} onClose={onClose} />,
    );

    fireEvent.change(screen.getByLabelText('Summary'), { target: { value: 'Renamed' } });
    fireEvent.click(screen.getByLabelText('All events'));
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/discards the changes/)).toBeNull();
  });
});
