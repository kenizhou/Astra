// SPDX-License-Identifier: GPL-3.0-only
// The calendar wire, pinned against the literal JSON astra-app's
// protocol_calendar_tests.rs and astra-viewmodel's calendar_tests.rs
// assert: every intent crosses as exactly the bytes the Rust side reads,
// the shared fixture parses as the snapshot on both sides of the wire,
// and the brief call answers its three states.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import type { CalendarSnapshot, EventDetail, MeetingBriefView } from './calendar';
import { calendarIntents } from './calendar';
import fixtureJson from './__fixtures__/calendar_snapshot.json';

const CALENDAR = { account: 'work', key: 'cal-1' };
const EVENT = { account: 'work', key: '/dav/cal/work/default/evt-1.ics' };
const MESSAGE = { account: 'work', key: 'imap:v1:u7@INBOX' };
const ZONED_START = {
  Zoned: { local: '2026-10-19T09:00:00', zone: { Iana: 'Europe/Amsterdam' } },
};
const ZONED_END = {
  Zoned: { local: '2026-10-19T09:30:00', zone: { Iana: 'Europe/Amsterdam' } },
};

// The shared fixture, byte-for-byte the JSON astra-viewmodel's
// calendar_tests.rs pins for a snapshot holding every row kind (the Rust
// suite reads this same file through its own include).
describe('the calendar snapshot wire', () => {
  it('carries every row kind with the recurrence markers and the series warning', () => {
    const snapshot = fixtureJson as CalendarSnapshot;

    expect(snapshot.calendars).toEqual([
      {
        account: 'work',
        id: 'cal-1',
        name: 'Work',
        color: '#039be5',
        visible: true,
        can_write: true,
        is_default: true,
      },
    ]);
    expect(snapshot.occurrences).toEqual([
      {
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
      },
    ]);
    expect(snapshot.write_states).toEqual([{ account: 'work', state: 'writing' }]);
    expect(snapshot.reply_prompts).toEqual([
      {
        account: 'work',
        message: 'imap:v1:u7@INBOX',
        summary: 'Sprint planning',
        organizer: 'boss@example.test',
        response: 'accepted',
        status_code: '5.2',
      },
    ]);
    expect(snapshot.now_data).toEqual({
      now: '2026-10-19T08:12:00Z',
      zone: 'Europe/Amsterdam',
    });
  });

  it('crosses an at-rest snapshot as an empty object', () => {
    const snapshot = JSON.parse('{}') as CalendarSnapshot;
    expect(snapshot.calendars).toBeUndefined();
    expect(snapshot.occurrences).toBeUndefined();
    expect(snapshot.write_states).toBeUndefined();
    expect(snapshot.reply_prompts).toBeUndefined();
    expect(snapshot.now_data).toBeUndefined();
  });

  it('pulls the snapshot by its command name', async () => {
    invoke.mockResolvedValue({});
    const { calendar } = await import('./calendar');
    await calendar();
    expect(invoke).toHaveBeenCalledWith('calendar');
  });
});

// Every literal is byte-for-byte the JSON astra-app's
// protocol_calendar_tests.rs asserts, so neither side of the wire can drift.
describe('the calendar intent wire', () => {
  it('crosses refresh and set date as their words', () => {
    expect(JSON.stringify(calendarIntents.refreshCalendar())).toBe('{"calendar":"refresh"}');
    expect(JSON.stringify(calendarIntents.setCalendarDate('2026-10-19'))).toBe(
      '{"calendar":{"set_date":{"date":"2026-10-19"}}}',
    );
  });

  it('carries the window anchor and its length', () => {
    expect(JSON.stringify(calendarIntents.setCalendarRange('2026-10-18T22:00:00Z', 7))).toBe(
      '{"calendar":{"set_range":{"start":"2026-10-18T22:00:00Z","days":7}}}',
    );
  });

  it('carries the view beside the window when the navigation names one', () => {
    expect(
      JSON.stringify(calendarIntents.setCalendarRange('2026-10-18T22:00:00Z', 7, 'work_week')),
    ).toBe(
      '{"calendar":{"set_range":{"start":"2026-10-18T22:00:00Z","days":7,"view":"work_week"}}}',
    );
  });

  it('names each visible calendar by reference', () => {
    expect(
      JSON.stringify(
        calendarIntents.setVisibleCalendars([
          { account: 'work', key: 'cal-1' },
          { account: 'home', key: 'personal' },
        ]),
      ),
    ).toBe(
      '{"calendar":{"set_visible_calendars":{"ids":[{"account":"work","key":"cal-1"},{"account":"home","key":"personal"}]}}}',
    );
  });

  it('creates an event with the engine-shaped fields, skipping what is absent', () => {
    expect(
      JSON.stringify(
        calendarIntents.createCalendarEvent({
          calendar: CALENDAR,
          summary: 'Standup',
          start: ZONED_START,
          end: ZONED_END,
        }),
      ),
    ).toBe(
      '{"calendar":{"create_event":{"calendar":{"account":"work","key":"cal-1"},"summary":"Standup","start":{"Zoned":{"local":"2026-10-19T09:00:00","zone":{"Iana":"Europe/Amsterdam"}}},"end":{"Zoned":{"local":"2026-10-19T09:30:00","zone":{"Iana":"Europe/Amsterdam"}}}}}}',
    );
    expect(
      JSON.stringify(
        calendarIntents.createCalendarEvent({
          calendar: CALENDAR,
          summary: 'Standup',
          start: ZONED_START,
          end: ZONED_END,
          description: 'Weekly sync',
          location: 'Room A',
          recurrence: {
            frequency: 'weekly',
            interval: 1,
            weekdays: ['mo'],
            end: { after_count: 5 },
          },
        }),
      ),
    ).toBe(
      '{"calendar":{"create_event":{"calendar":{"account":"work","key":"cal-1"},"summary":"Standup","start":{"Zoned":{"local":"2026-10-19T09:00:00","zone":{"Iana":"Europe/Amsterdam"}}},"end":{"Zoned":{"local":"2026-10-19T09:30:00","zone":{"Iana":"Europe/Amsterdam"}}},"description":"Weekly sync","location":"Room A","recurrence":{"frequency":"weekly","interval":1,"weekdays":["mo"],"end":{"after_count":5}}}}}',
    );
    expect(
      JSON.stringify(
        calendarIntents.createCalendarEvent({
          calendar: CALENDAR,
          summary: '',
          start: { Date: '2026-10-19' },
          end: { Date: '2026-10-20' },
          recurrence: { frequency: 'daily', interval: 2, weekdays: [], end: 'never' },
        }),
      ),
    ).toBe(
      '{"calendar":{"create_event":{"calendar":{"account":"work","key":"cal-1"},"summary":"","start":{"Date":"2026-10-19"},"end":{"Date":"2026-10-20"},"recurrence":{"frequency":"daily","interval":2,"weekdays":[],"end":"never"}}}}',
    );
    expect(
      JSON.stringify(
        calendarIntents.createCalendarEvent({
          calendar: CALENDAR,
          summary: '',
          start: { Date: '2026-10-19' },
          end: { Date: '2026-10-20' },
          recurrence: {
            frequency: 'daily',
            interval: 2,
            weekdays: [],
            end: { on_date: '2026-12-18' },
          },
        }),
      ),
    ).toBe(
      '{"calendar":{"create_event":{"calendar":{"account":"work","key":"cal-1"},"summary":"","start":{"Date":"2026-10-19"},"end":{"Date":"2026-10-20"},"recurrence":{"frequency":"daily","interval":2,"weekdays":[],"end":{"on_date":"2026-12-18"}}}}}',
    );
  });

  it('edits an event in its own wall clock, skipping untouched fields', () => {
    expect(JSON.stringify(calendarIntents.updateCalendarEvent(EVENT, { title: 'Renamed' }))).toBe(
      '{"calendar":{"update_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},"edit":{"title":"Renamed"}}}}',
    );
    expect(
      JSON.stringify(
        calendarIntents.updateCalendarEvent(EVENT, {
          start: '2026-10-20T09:30:00',
          end: '2026-10-20T10:00:00',
          notes: '',
          location: 'Room B',
          recurrence: 'clear',
          occurrence: '2026-10-20T09:00:00',
        }),
      ),
    ).toBe(
      '{"calendar":{"update_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},"edit":{"start":"2026-10-20T09:30:00","end":"2026-10-20T10:00:00","notes":"","location":"Room B","recurrence":"clear","occurrence":"2026-10-20T09:00:00"}}}}',
    );
    expect(
      JSON.stringify(
        calendarIntents.updateCalendarEvent(EVENT, {
          start: '2026-10-20T09:30:00',
          end: '2026-10-20T10:00:00',
          times_from_occurrence: '2026-10-20T09:00:00',
        }),
      ),
    ).toBe(
      '{"calendar":{"update_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},"edit":{"start":"2026-10-20T09:30:00","end":"2026-10-20T10:00:00","times_from_occurrence":"2026-10-20T09:00:00"}}}}',
    );
  });

  it('carries the drag in the grid own currency, all three edges', () => {
    expect(
      JSON.stringify(
        calendarIntents.moveCalendarEvent(EVENT, {
          edge: 'whole',
          days: 1,
          minutes: -30,
          occurrence: '2026-10-20T09:00:00',
        }),
      ),
    ).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},"drag":{"edge":"whole","days":1,"minutes":-30,"occurrence":"2026-10-20T09:00:00"}}}}',
    );
    expect(
      JSON.stringify(calendarIntents.moveCalendarEvent(EVENT, { edge: 'end', days: 0, minutes: 45 })),
    ).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},"drag":{"edge":"end","days":0,"minutes":45}}}}',
    );
    expect(
      JSON.stringify(calendarIntents.moveCalendarEvent(EVENT, { edge: 'start', days: 0, minutes: 15 })),
    ).toBe(
      '{"calendar":{"move_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},"drag":{"edge":"start","days":0,"minutes":15}}}}',
    );
  });

  it('deletes the series or the one occurrence', () => {
    expect(JSON.stringify(calendarIntents.deleteCalendarEvent(EVENT))).toBe(
      '{"calendar":{"delete_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"}}}}',
    );
    expect(JSON.stringify(calendarIntents.deleteCalendarEvent(EVENT, '2026-10-20T09:00:00'))).toBe(
      '{"calendar":{"delete_event":{"event":{"account":"work","key":"/dav/cal/work/default/evt-1.ics"},"occurrence":"2026-10-20T09:00:00"}}}',
    );
  });

  it('answers an invitation with the note and the notify flag both ways', () => {
    expect(JSON.stringify(calendarIntents.respondToInvitation(MESSAGE, 'accepted'))).toBe(
      '{"calendar":{"respond_to_invitation":{"message":{"account":"work","key":"imap:v1:u7@INBOX"},"response":"accepted","notify_organizer":true}}}',
    );
    expect(
      JSON.stringify(
        calendarIntents.respondToInvitation(MESSAGE, 'declined', 'Clashes with the offsite', false),
      ),
    ).toBe(
      '{"calendar":{"respond_to_invitation":{"message":{"account":"work","key":"imap:v1:u7@INBOX"},"response":"declined","comment":"Clashes with the offsite","notify_organizer":false}}}',
    );
    expect(JSON.stringify(calendarIntents.respondToInvitation(MESSAGE, 'tentative'))).toBe(
      '{"calendar":{"respond_to_invitation":{"message":{"account":"work","key":"imap:v1:u7@INBOX"},"response":"tentative","notify_organizer":true}}}',
    );
  });

  it('answers the reply prompt with the decision and the optional subject', () => {
    expect(JSON.stringify(calendarIntents.answerReplyPrompt(true, false))).toBe(
      '{"calendar":{"answer_reply_prompt":{"send":true,"remember":false}}}',
    );
    expect(
      JSON.stringify(calendarIntents.answerReplyPrompt(false, true, 'Zusage: Sprint planning')),
    ).toBe(
      '{"calendar":{"answer_reply_prompt":{"send":false,"remember":true,"reply_subject":"Zusage: Sprint planning"}}}',
    );
  });
});

// The detail literal is byte-for-byte the JSON astra-desktop's
// calendar_tests.rs asserts for the roll-call and the rule read, so the
// folded tail and the sentence cannot drift between the host and the
// surface.
describe('the event detail wire', () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it('mirrors the roll-call and the rule the core answers', () => {
    const wire = JSON.parse(
      '{"account":"work","event":"/dav/cal/work/default/evt-1.ics",' +
        '"participants":[{"name_or_address":"Ada Lovelace","role":"organiser","response":"accepted"},' +
        '{"name_or_address":"blaise@example.test","response":"needs_action"}],' +
        '"folded_participants":4,' +
        '"rule":{"frequency":"weekly","interval":1,"sentence":"Repeats every week."}}',
    ) as EventDetail;

    expect(wire.participants).toEqual([
      {
        name_or_address: 'Ada Lovelace',
        role: 'organiser',
        response: 'accepted',
      },
      { name_or_address: 'blaise@example.test', response: 'needs_action' },
    ]);
    expect(wire.folded_participants).toBe(4);
    expect(wire.rule).toEqual({
      frequency: 'weekly',
      interval: 1,
      sentence: 'Repeats every week.',
    });
  });

  it('asks the host for one event own detail by its reference', async () => {
    invoke.mockResolvedValue(undefined);
    const { calendarEventDetail } = await import('./calendar');
    await calendarEventDetail('work', '/dav/cal/work/default/evt-1.ics');
    expect(invoke).toHaveBeenCalledWith('calendar_event_detail', {
      account: 'work',
      key: '/dav/cal/work/default/evt-1.ics',
    });
  });
});

// Both literals are byte-for-byte the JSON astra-app's
// calendar_brief_tests.rs and astra-desktop's calendar_tests.rs assert,
// so the brief's three states cannot drift between the host and the
// surface.
describe('the meeting brief wire', () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it('mirrors the generated view with its per-meeting rows', () => {
    const wire = JSON.parse(
      '{"account":"work","day":"2026-10-19","state":"generated","brief":"The standup opens the day; the planning needs the room booking confirmed.","meetings":[{"title":"Sprint planning","start":"2026-10-19T12:00:00Z","end":"2026-10-19T13:00:00Z","all_day":false,"organizer":"Boss <boss@test.local>","attendees":["Info <info@test.local>"],"location":"Room A","invitation_subject":"Sprint planning"}]}',
    ) as MeetingBriefView;

    expect(wire.account).toBe('work');
    expect(wire.day).toBe('2026-10-19');
    expect(wire.state).toBe('generated');
    expect(wire.brief).toBe(
      'The standup opens the day; the planning needs the room booking confirmed.',
    );
    expect(wire.meetings).toEqual([
      {
        title: 'Sprint planning',
        start: '2026-10-19T12:00:00Z',
        end: '2026-10-19T13:00:00Z',
        all_day: false,
        organizer: 'Boss <boss@test.local>',
        attendees: ['Info <info@test.local>'],
        location: 'Room A',
        invitation_subject: 'Sprint planning',
      },
    ]);
  });

  it('mirrors the no-meetings view with no brief and no rows on the wire', () => {
    const raw = '{"account":"work","day":"2026-10-26","state":"no_meetings"}';
    expect(raw).not.toContain('"brief"');
    expect(raw).not.toContain('"meetings"');
    const wire = JSON.parse(raw) as MeetingBriefView;
    expect(wire.state).toBe('no_meetings');
    expect(wire.brief).toBeUndefined();
    expect(wire.meetings).toBeUndefined();
  });

  it('asks the host for the brief by account and day', async () => {
    invoke.mockResolvedValue({ account: 'work', day: '2026-10-19', state: 'no_meetings' });
    const { calendarBriefGenerate } = await import('./calendar');
    await calendarBriefGenerate('work', '2026-10-19');
    expect(invoke).toHaveBeenCalledWith('calendar_brief_generate', {
      account: 'work',
      day: '2026-10-19',
    });
  });

  it('surfaces a refused generation as the rejected promise, never a panic', async () => {
    invoke.mockRejectedValue('the ai surface refused: no ai bundle is running');
    const { calendarBriefGenerate } = await import('./calendar');
    await expect(calendarBriefGenerate('work', '2026-10-19')).rejects.toBe(
      'the ai surface refused: no ai bundle is running',
    );
  });
});
