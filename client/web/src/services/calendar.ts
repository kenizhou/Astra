// SPDX-License-Identifier: GPL-3.0-only
// The calendar half of the IPC wire: the CalendarSnapshot mirror (the
// manager set, the joined occurrence rows with their recurrence markers
// and the series warning, the write states, the reply prompts, and the
// now line), the CalendarIntent family's constructors, the snapshot
// pull, and the meeting brief call. Split from ipc.ts to keep that file
// under the line cap, like the compose, sending, and setup halves before
// it, and re-exported whole through it, so ipc stays the one import
// point of the wire. The literals are pinned in calendar.test.ts against
// the JSON astra-app's protocol_calendar_tests.rs and astra-viewmodel's
// calendar_tests.rs assert.

import { invoke } from '@tauri-apps/api/core';

import type { MessageRef } from './ipc';

/** A calendar bound to its owning account; unique only within the account. */
export interface CalendarRef {
  account: string;
  key: string;
}

/** An event bound to its owning account by its provider key. */
export interface EventRef {
  account: string;
  key: string;
}

/** A zone as the engine states it, crossing verbatim and externally tagged. */
export type TimeZoneId = { Iana: string } | { Custom: string };

/**
 * A time as the engine states it, crossing verbatim in the engine's own
 * representation: a zoned wall clock, a floating one, or a bare date.
 */
export type CalendarDateTime =
  | { Zoned: { local: string; zone: TimeZoneId } }
  | { Floating: string }
  | { Date: string };

/** How often a repeat rule fires; crosses as a snake_case word. */
export type Frequency =
  | 'yearly'
  | 'monthly'
  | 'weekly'
  | 'daily'
  | 'hourly'
  | 'minutely'
  | 'secondly';

/** A weekday a repeat rule names; crosses as a snake_case word. */
export type Weekday = 'mo' | 'tu' | 'we' | 'th' | 'fr' | 'sa' | 'su';

/** A repeat rule as the host can state it, the editor's subset. */
export interface RepeatRule {
  frequency: Frequency;
  interval: number;
  weekdays: Weekday[];
  end: RepeatEnd;
}

/** When a repeat rule stops, alone or with its bound. */
export type RepeatEnd = 'never' | { on_date: string } | { after_count: number };

/** What an edit does to an event's repeat rule. */
export type RecurrenceChange = { set: RepeatRule } | 'clear';

/** Which edges of an event a drag moved; crosses as a snake_case word. */
export type EventEdge = 'whole' | 'start' | 'end';

/** One drag of an event on the grid, in the grid's own currency. */
export interface EventDrag {
  edge: EventEdge;
  days: number;
  minutes: number;
  /** The dragged occurrence, named by its original start; absent moves the series. */
  occurrence?: string;
}

/** An edit to a stored event, in the event's own wall clock. */
export interface CalendarEventEdit {
  title?: string;
  start?: string;
  end?: string;
  notes?: string;
  location?: string;
  recurrence?: RecurrenceChange;
  /** The one occurrence this edits; absent edits the whole series. */
  occurrence?: string;
  /** The occurrence the editor's clocks were read from, for a series edit. */
  times_from_occurrence?: string;
}

/** The fields a create carries: the user-chosen half of the engine's draft. */
export interface CalendarEventCreate {
  calendar: CalendarRef;
  summary: string;
  start: CalendarDateTime;
  end: CalendarDateTime;
  description?: string;
  location?: string;
  recurrence?: RepeatRule;
}

/** The answer a user can give to an invitation; crosses as a snake_case word. */
export type RsvpAnswer = 'accepted' | 'tentative' | 'declined';

/**
 * What the calendar surface asks the runtime to do, nested under the
 * `calendar` tag of the intent that carries it. Built through the
 * `calendarIntents` constructors so the key order matches the Rust wire
 * byte for byte.
 */
export type CalendarIntentBody =
  | 'refresh'
  | { set_range: { start: string; days: number } }
  | { set_visible_calendars: { ids: CalendarRef[] } }
  | { create_event: CalendarEventCreate }
  | { update_event: { event: EventRef; edit: CalendarEventEdit } }
  | { move_event: { event: EventRef; drag: EventDrag } }
  | { delete_event: { event: EventRef; occurrence?: string } }
  | {
      respond_to_invitation: {
        message: MessageRef;
        response: RsvpAnswer;
        comment?: string;
        notify_organizer: boolean;
      };
    }
  | { answer_reply_prompt: { send: boolean; remember: boolean; reply_subject?: string } }
  | { set_date: { date: string } };

/** The calendar constructors, spread into ipc's `intents`, the family in the enum's own order. */
export const calendarIntents = {
  /** Sync the account's calendars and refresh the calendar snapshot. */
  refreshCalendar: () => ({ calendar: 'refresh' as const }),
  /** Move the visible window: the UTC instant it opens at and its length in days. */
  setCalendarRange: (start: string, days: number) => ({
    calendar: { set_range: { start, days } },
  }),
  /** Name which calendars' events are drawn; an empty set hides every calendar's. */
  setVisibleCalendars: (ids: CalendarRef[]) => ({
    calendar: { set_visible_calendars: { ids } },
  }),
  /** Create an event, in the engine's own create shape. */
  createCalendarEvent: (event: CalendarEventCreate) => ({
    calendar: {
      create_event: {
        calendar: event.calendar,
        summary: event.summary,
        start: event.start,
        end: event.end,
        ...(event.description !== undefined && { description: event.description }),
        ...(event.location !== undefined && { location: event.location }),
        ...(event.recurrence !== undefined && { recurrence: event.recurrence }),
      },
    },
  }),
  /** Edit a stored event, in the event's own wall clock. */
  updateCalendarEvent: (event: EventRef, edit: CalendarEventEdit) => ({
    calendar: {
      update_event: {
        event,
        edit: {
          ...(edit.title !== undefined && { title: edit.title }),
          ...(edit.start !== undefined && { start: edit.start }),
          ...(edit.end !== undefined && { end: edit.end }),
          ...(edit.notes !== undefined && { notes: edit.notes }),
          ...(edit.location !== undefined && { location: edit.location }),
          ...(edit.recurrence !== undefined && { recurrence: edit.recurrence }),
          ...(edit.occurrence !== undefined && { occurrence: edit.occurrence }),
          ...(edit.times_from_occurrence !== undefined && {
            times_from_occurrence: edit.times_from_occurrence,
          }),
        },
      },
    },
  }),
  /** Move or resize an event by a drag, in the grid's own currency. */
  moveCalendarEvent: (event: EventRef, drag: EventDrag) => ({
    calendar: {
      move_event: {
        event,
        drag: {
          edge: drag.edge,
          days: drag.days,
          minutes: drag.minutes,
          ...(drag.occurrence !== undefined && { occurrence: drag.occurrence }),
        },
      },
    },
  }),
  /** Delete an event, or one occurrence of it. */
  deleteCalendarEvent: (event: EventRef, occurrence?: string) => ({
    calendar: {
      delete_event: {
        event,
        ...(occurrence !== undefined && { occurrence }),
      },
    },
  }),
  /** Answer a meeting invitation: the message the reading pane is showing. */
  respondToInvitation: (
    message: MessageRef,
    response: RsvpAnswer,
    comment?: string,
    notifyOrganizer = true,
  ) => ({
    calendar: {
      respond_to_invitation: {
        message,
        response,
        ...(comment !== undefined && { comment }),
        notify_organizer: notifyOrganizer,
      },
    },
  }),
  /** Answer the reply-delivery-failure prompt. */
  answerReplyPrompt: (send: boolean, remember: boolean, replySubject?: string) => ({
    calendar: {
      answer_reply_prompt: {
        send,
        remember,
        ...(replySubject !== undefined && { reply_subject: replySubject }),
      },
    },
  }),
  /** Move the grids' navigation anchor to a date (the jump-to-today). */
  setCalendarDate: (date: string) => ({ calendar: { set_date: { date } } }),
};

/** One calendar a user can see, colour, and toggle. */
export interface CalendarRow {
  account: string;
  id: string;
  name: string;
  color?: string;
  visible: boolean;
  can_write: boolean;
  is_default: boolean;
}

/** How an occurrence row's instants were resolved; a snake_case word. */
export type OccurrenceAccuracy = 'zoned' | 'floating' | 'all_day';

/** How this account stands in a meeting; a snake_case word. */
export type Participation =
  | 'needs_action'
  | 'accepted'
  | 'declined'
  | 'tentative'
  | 'delegated';

/** One materialized occurrence, joined to the master event that carries its content. */
export interface OccurrenceRow {
  account: string;
  event: string;
  calendar: string;
  title: string;
  start: string;
  end: string;
  all_day: boolean;
  recurring: boolean;
  overridden: boolean;
  occurrence_start?: string;
  accuracy: OccurrenceAccuracy;
  participation: Participation;
  can_move: boolean;
  series_edit_warns: boolean;
}

/** One account's calendar write, as the write indicator shows it. */
export interface CalendarWriteRow {
  account: string;
  state: 'writing' | 'failed_with_retry';
}

/** One invitation whose server could not deliver the answer: the prompt. */
export interface ReplyPromptRow {
  account: string;
  message: string;
  summary: string;
  organizer: string;
  response: Participation;
  comment?: string;
  status_code: string;
}

/** What the now line draws from. */
export interface NowLineData {
  now: string;
  zone: string;
}

/**
 * The invitation card above the reading pane's body: what the open message
 * means to this account's calendar. The words are the two-condition gate's,
 * the same verdict the answer path decides through.
 */
export interface InvitationBlock {
  /** The gate's verdict, picking which affordances the card offers. */
  kind: 'rsvp' | 'cancelled' | 'informational' | 'superseded';
  /** How this account stands in the meeting as the calendar holds it. */
  response: Participation;
  /** Whether the calendar holds a newer revision than the copy on screen. */
  stale: boolean;
  /** The meeting's title, as the invitation carries it. */
  summary: string;
  /** The organiser's address: who an answer reaches. */
  organizer: string;
}

/** The calendar snapshot the surface renders after every pull. */
export interface CalendarSnapshot {
  calendars?: CalendarRow[];
  occurrences?: OccurrenceRow[];
  write_states?: CalendarWriteRow[];
  reply_prompts?: ReplyPromptRow[];
  now_data?: NowLineData;
}

/** Where one brief of one day stands; a snake_case word. */
export type MeetingBriefState = 'generated' | 'no_meetings';

/** One meeting row of the brief view. */
export interface BriefMeetingRow {
  title: string;
  start: string;
  end: string;
  all_day: boolean;
  organizer?: string;
  attendees?: string[];
  location?: string;
  invitation_subject?: string;
}

/** One brief of one account's civil day, as the brief command answers it. */
export interface MeetingBriefView {
  account: string;
  day: string;
  state: MeetingBriefState;
  brief?: string;
  meetings?: BriefMeetingRow[];
}

/** Pulls the calendar snapshot. */
export function calendar(): Promise<CalendarSnapshot> {
  return invoke('calendar');
}

/** Generates the account's meeting brief for the civil day, answered fresh every ask. */
export function calendarBriefGenerate(account: string, day: string): Promise<MeetingBriefView> {
  return invoke('calendar_brief_generate', { account, day });
}
