// SPDX-License-Identifier: GPL-3.0-only
// The calendar store: the last pulled snapshot's row sets, the view kind the
// toolbar switches, and the navigation anchor seeded once from the snapshot's
// own now data so a cold calendar opens on today in the display zone.
import { beforeEach, describe, expect, it } from 'vitest';

import { useCalendarStore } from './calendarStore';
import type { CalendarSnapshot, OccurrenceRow } from '@/services/ipc';

/** An occurrence row of the fixture's shape, overridable per test. */
function occurrence(over: Partial<OccurrenceRow> = {}): OccurrenceRow {
  return {
    account: 'work',
    event: '/dav/cal/work/default/evt-1.ics',
    calendar: 'cal-1',
    title: 'Standup',
    start: '2026-10-19T07:00:00Z',
    end: '2026-10-19T07:30:00Z',
    all_day: false,
    recurring: true,
    overridden: true,
    occurrence_start: '2026-10-19T09:00:00',
    accuracy: 'zoned',
    participation: 'tentative',
    can_move: false,
    series_edit_warns: true,
    ...over,
  };
}

beforeEach(() => {
  useCalendarStore.setState({
    calendars: [],
    occurrences: [],
    writeStates: [],
    replyPrompts: [],
    nowData: null,
    view: 'month',
    anchor: null,
    selectedEvent: null,
  });
});

describe('the calendar store apply', () => {
  it('lands every row set and the now data of the snapshot', () => {
    const snapshot: CalendarSnapshot = {
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
      occurrences: [occurrence()],
      write_states: [{ account: 'work', state: 'writing' }],
      reply_prompts: [
        {
          account: 'work',
          message: 'imap:v1:u7@INBOX',
          summary: 'Sprint planning',
          organizer: 'boss@example.test',
          response: 'accepted',
          status_code: '5.2',
        },
      ],
      now_data: { now: '2026-10-19T08:12:00Z', zone: 'Europe/Amsterdam' },
    };

    useCalendarStore.getState().applySnapshot(snapshot);

    const state = useCalendarStore.getState();
    expect(state.calendars).toEqual(snapshot.calendars);
    expect(state.occurrences).toEqual(snapshot.occurrences);
    expect(state.writeStates).toEqual(snapshot.write_states);
    expect(state.replyPrompts).toEqual(snapshot.reply_prompts);
    expect(state.nowData).toEqual({ now: '2026-10-19T08:12:00Z', zone: 'Europe/Amsterdam' });
  });

  it('seeds the anchor from the zone-local today of the first now data', () => {
    // Half past ten at night in UTC is already the 20th in Amsterdam.
    useCalendarStore.getState().applySnapshot({
      now_data: { now: '2026-10-19T22:30:00Z', zone: 'Europe/Amsterdam' },
    });

    expect(useCalendarStore.getState().anchor).toBe('2026-10-20');
  });

  it('never moves a standing anchor on a later snapshot', () => {
    useCalendarStore.getState().applySnapshot({
      now_data: { now: '2026-10-19T22:30:00Z', zone: 'Europe/Amsterdam' },
    });
    useCalendarStore.getState().setAnchor('2026-11-02');

    useCalendarStore.getState().applySnapshot({
      now_data: { now: '2026-12-24T10:00:00Z', zone: 'Europe/Amsterdam' },
    });

    expect(useCalendarStore.getState().anchor).toBe('2026-11-02');
  });

  it('keeps the anchor it holds when a snapshot carries no now data', () => {
    useCalendarStore.getState().setAnchor('2026-10-19');

    useCalendarStore.getState().applySnapshot({});

    expect(useCalendarStore.getState().anchor).toBe('2026-10-19');
    expect(useCalendarStore.getState().nowData).toBeNull();
    expect(useCalendarStore.getState().calendars).toEqual([]);
  });

  it('records the view the toolbar switches to', () => {
    expect(useCalendarStore.getState().view).toBe('month');

    useCalendarStore.getState().setView('work_week');
    expect(useCalendarStore.getState().view).toBe('work_week');

    useCalendarStore.getState().setView('agenda');
    expect(useCalendarStore.getState().view).toBe('agenda');
  });
});

describe('the calendar store shape restore', () => {
  it('seeds the view and a past anchor from the snapshot own shape', () => {
    // A launch whose preference left the agenda standing on a September
    // day: the store restores that shape exactly, not today in the month.
    useCalendarStore.getState().applySnapshot({
      shape: { view: 'agenda', anchor: '2026-09-15' },
      now_data: { now: '2026-10-19T22:30:00Z', zone: 'Europe/Amsterdam' },
    });

    expect(useCalendarStore.getState().view).toBe('agenda');
    expect(useCalendarStore.getState().anchor).toBe('2026-09-15');
  });

  it('falls back to the month and the zone-local today when the shape holds no anchor', () => {
    useCalendarStore.getState().applySnapshot({
      shape: { view: 'month' },
      now_data: { now: '2026-10-19T22:30:00Z', zone: 'Europe/Amsterdam' },
    });

    expect(useCalendarStore.getState().view).toBe('month');
    expect(useCalendarStore.getState().anchor).toBe('2026-10-20');
  });

  it('follows the published shape, so a refused write keeps the standing view', () => {
    useCalendarStore.getState().applySnapshot({ shape: { view: 'month' } });

    // The optimistic switch a navigation makes...
    useCalendarStore.getState().setView('day');
    expect(useCalendarStore.getState().view).toBe('day');

    // ...and the publish after the refused write still carries the standing
    // shape, which the store follows back.
    useCalendarStore.getState().applySnapshot({ shape: { view: 'month' } });
    expect(useCalendarStore.getState().view).toBe('month');
  });

  it('keeps a standing anchor over a later shape the navigation did not move', () => {
    useCalendarStore.getState().applySnapshot({
      shape: { view: 'month', anchor: '2026-09-15' },
    });
    useCalendarStore.getState().setAnchor('2026-11-02');

    useCalendarStore.getState().applySnapshot({
      shape: { view: 'month', anchor: '2026-09-15' },
      now_data: { now: '2026-12-24T10:00:00Z', zone: 'Europe/Amsterdam' },
    });

    // The anchor is the user's place in the diary, moved only by
    // navigation; the shape's anchor seeds the boot, never a later publish.
    expect(useCalendarStore.getState().anchor).toBe('2026-11-02');
  });
});

describe('the calendar store selection', () => {
  it('picks an occurrence and clears it on a second pick', () => {
    const row = occurrence();

    useCalendarStore.getState().selectEvent(row);
    expect(useCalendarStore.getState().selectedEvent).toBe(row);

    useCalendarStore.getState().selectEvent(row);
    expect(useCalendarStore.getState().selectedEvent).toBeNull();
  });

  it('clears the pick on navigation', () => {
    useCalendarStore.getState().selectEvent(occurrence());

    useCalendarStore.getState().setAnchor('2026-11-02');
    expect(useCalendarStore.getState().selectedEvent).toBeNull();

    useCalendarStore.getState().selectEvent(occurrence());
    useCalendarStore.getState().setView('day');
    expect(useCalendarStore.getState().selectedEvent).toBeNull();
  });

  it('tracks the picked row across a publish and drops it when it leaves the window', () => {
    const stale = occurrence({ title: 'Old words' });
    useCalendarStore.setState({ occurrences: [stale] });
    useCalendarStore.getState().selectEvent(stale);

    const fresh = occurrence({ title: 'New words' });
    useCalendarStore.getState().applySnapshot({ occurrences: [fresh] });
    expect(useCalendarStore.getState().selectedEvent).toBe(fresh);

    useCalendarStore.getState().applySnapshot({ occurrences: [] });
    expect(useCalendarStore.getState().selectedEvent).toBeNull();
  });
});
