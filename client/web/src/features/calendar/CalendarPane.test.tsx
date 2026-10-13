// SPDX-License-Identifier: GPL-3.0-only
// The calendar pane: the toolbar over the resizable pane row (rail, grid,
// and the detail dock a pick opens), the write banners the write states
// draw, the reply-delivery prompt riding above the view, the event editor
// the New event action and a drag-create open, and the refresh on entry and
// on focus regain.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// jsdom has no ResizeObserver; the resizable panel row measures through one.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', ResizeObserverStub);

const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), calendarBriefGenerate: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  calendarBriefGenerate: ipc.calendarBriefGenerate,
}));

import { CalendarPane } from './CalendarPane';
import { useCalendarStore } from './calendarStore';
import type { OccurrenceRow } from '@/services/ipc';

/** A timed occurrence row at 09:00 zone time on 19 October 2026. */
function occurrence(over: Partial<OccurrenceRow> = {}): OccurrenceRow {
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
    selectedEvent: null,
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

describe('the pane row', () => {
  it('lays out the rail and the grid with a resize handle', () => {
    render(<CalendarPane />);

    expect(screen.getByLabelText('Calendar rail')).toBeTruthy();
    expect(screen.getByLabelText('Monday 28 September 2026')).toBeTruthy();
    expect(screen.getAllByRole('separator')).toHaveLength(1);
  });

  it('moves the anchor from the mini-month without leaving the month view', () => {
    render(<CalendarPane />);

    fireEvent.click(screen.getByRole('button', { name: 'Go to 2026-10-20' }));

    expect(useCalendarStore.getState().anchor).toBe('2026-10-20');
    expect(useCalendarStore.getState().view).toBe('month');
    expect(JSON.stringify(ipc.dispatch.mock.calls.at(-2)?.[0])).toBe(
      '{"calendar":{"set_date":{"date":"2026-10-20"}}}',
    );
  });

  it('toggles a calendar from the rail checkbox with the manager intent', () => {
    render(<CalendarPane />);

    fireEvent.click(screen.getByRole('checkbox', { name: 'Show Work' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls.at(-1)?.[0])).toBe(
      '{"calendar":{"set_visible_calendars":{"ids":[]}}}',
    );
  });
});

describe('the event detail dock', () => {
  it('picks an event from its chip, opening the dock rather than the editor', () => {
    useCalendarStore.setState({ occurrences: [occurrence()] });
    render(<CalendarPane />);

    fireEvent.click(screen.getByText('Standup'));

    expect(screen.queryByRole('dialog')).toBeNull();
    const dock = screen.getByLabelText('Event detail');
    expect(dock.textContent).toContain('Standup');
    expect(dock.textContent).toContain('Monday 19 October 2026, 9 AM to 9:30 AM');
    expect(dock.textContent).toContain('Work');
    expect(screen.getAllByRole('separator')).toHaveLength(2);
  });

  it('clears the pick on a second click and on Escape', () => {
    useCalendarStore.setState({ occurrences: [occurrence()] });
    render(<CalendarPane />);

    fireEvent.click(screen.getByRole('button', { name: 'Standup' }));
    expect(screen.getByLabelText('Event detail')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Standup' }));
    expect(screen.queryByLabelText('Event detail')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Standup' }));
    fireEvent.keyDown(screen.getByRole('region', { name: 'Calendar' }), { key: 'Escape' });
    expect(screen.queryByLabelText('Event detail')).toBeNull();
  });

  it('opens the editor from the dock Edit action', () => {
    useCalendarStore.setState({ occurrences: [occurrence()] });
    render(<CalendarPane />);

    fireEvent.click(screen.getByText('Standup'));
    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    expect(screen.getByRole('dialog', { name: 'Edit event' })).toBeTruthy();
    expect(screen.queryByLabelText('Event detail')).toBeNull();
  });
});

describe('drag-create', () => {
  beforeEach(() => {
    useCalendarStore.setState({ view: 'day', anchor: '2026-10-19' });
  });

  it('opens the editor prefilled with the slot dragged over empty time', () => {
    render(<CalendarPane />);

    const grid = screen.getByLabelText('Day grid, Monday 19 October 2026');
    fireEvent.pointerDown(grid, { pointerId: 1, clientX: 60, clientY: 396 });
    fireEvent.pointerMove(grid, { pointerId: 1, clientX: 60, clientY: 440 });
    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 60, clientY: 440 });

    expect(screen.getByRole('dialog', { name: 'New event' })).toBeTruthy();
    expect((screen.getByLabelText('Start') as HTMLInputElement).value).toBe('2026-10-19T09:00');
    expect((screen.getByLabelText('End') as HTMLInputElement).value).toBe('2026-10-19T10:00');
    expect(ipc.dispatch.mock.calls.some((call) => JSON.stringify(call[0]).includes('move_event'))).toBe(false);
  });

  it('still moves an event whose drag begins on its block', () => {
    useCalendarStore.setState({ occurrences: [occurrence()] });
    render(<CalendarPane />);

    const block = screen.getByText('Standup').closest('[data-block]')!;
    const grid = screen.getByLabelText('Day grid, Monday 19 October 2026');
    fireEvent.pointerDown(block, { pointerId: 1, clientX: 60, clientY: 396 });
    fireEvent.pointerUp(grid, { pointerId: 1, clientX: 60, clientY: 407 });

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(JSON.stringify(ipc.dispatch.mock.calls.at(-1)?.[0])).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},' +
        '"drag":{"edge":"whole","days":0,"minutes":15}}}}',
    );
  });
});
