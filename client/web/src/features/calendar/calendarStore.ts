// SPDX-License-Identifier: GPL-3.0-only
// The calendar surface's state: the last pulled snapshot's row sets (the
// calendar manager, the joined occurrences, the write states, the reply
// prompts, the now data), the view kind, and the navigation anchor. The
// view follows the snapshot's published shape so the one a past launch
// left restores at boot and a refused write keeps the standing shape; the
// anchor seeds once from the shape the person left (today in the display
// zone when no anchor stands) and after that only navigation moves it.

import { create } from 'zustand';

import type {
  CalendarRow,
  CalendarSnapshot,
  CalendarWriteRow,
  NowLineData,
  OccurrenceRow,
  ReplyPromptRow,
} from '@/services/ipc';

import { zoneToday } from './calendarRange';

/** The view kinds the calendar pane lays its occurrences out as. */
export type CalendarView = 'agenda' | 'day' | 'work_week' | 'month';

interface CalendarStoreState {
  /** The calendar manager set, one row per calendar every account holds. */
  calendars: CalendarRow[];
  /** The visible window's joined occurrence rows, ascending by start. */
  occurrences: OccurrenceRow[];
  /** The accounts with a calendar write in flight or unconfirmed. */
  writeStates: CalendarWriteRow[];
  /** The answers whose reply the server could not deliver: the prompts. */
  replyPrompts: ReplyPromptRow[];
  /** What the now line draws from; null until a snapshot carried it. */
  nowData: NowLineData | null;
  /** The view kind the pane renders. */
  view: CalendarView;
  /** The navigation anchor, a civil day key in the snapshot's zone. */
  anchor: string | null;
  /** The occurrence the detail dock shows, or null while nothing is picked. */
  selectedEvent: OccurrenceRow | null;
  /** Applies a pulled snapshot. */
  applySnapshot: (snapshot: CalendarSnapshot) => void;
  /** Switches the view kind; a view switch is navigation and clears the pick. */
  setView: (view: CalendarView) => void;
  /** Moves the navigation anchor to a civil day key; navigation clears the pick. */
  setAnchor: (day: string) => void;
  /** Picks an occurrence for the detail dock; picking it again clears it. */
  selectEvent: (row: OccurrenceRow) => void;
  /** Clears the picked occurrence. */
  clearSelection: () => void;
}

/** Whether two rows name the same occurrence. */
function sameOccurrence(a: OccurrenceRow, b: OccurrenceRow): boolean {
  return a.account === b.account && a.event === b.event && a.start === b.start;
}

export const useCalendarStore = create<CalendarStoreState>((set) => ({
  calendars: [],
  occurrences: [],
  writeStates: [],
  replyPrompts: [],
  nowData: null,
  view: 'month',
  anchor: null,
  selectedEvent: null,

  applySnapshot: (snapshot) =>
    set((state) => {
      const occurrences = snapshot.occurrences ?? [];
      return {
        calendars: snapshot.calendars ?? [],
        occurrences,
        writeStates: snapshot.write_states ?? [],
        replyPrompts: snapshot.reply_prompts ?? [],
        nowData: snapshot.now_data ?? null,
        // The published shape is the standing one: the store follows the
        // view every publish (an optimistic switch a refused write never
        // confirmed reverts with the next publish), while the anchor stays
        // the user's own place in the diary, seeded once from the shape a
        // past launch left and moved only by navigation after that.
        view: snapshot.shape?.view ?? state.view,
        anchor:
          state.anchor ??
          snapshot.shape?.anchor ??
          (snapshot.now_data ? zoneToday(snapshot.now_data.now, snapshot.now_data.zone) : null),
        // The pick tracks the publish: the fresh row replaces the stale one,
        // and a row the window no longer holds drops the pick with it.
        selectedEvent:
          state.selectedEvent === null
            ? null
            : (occurrences.find((row) => sameOccurrence(row, state.selectedEvent!)) ?? null),
      };
    }),

  setView: (view) => set({ view, selectedEvent: null }),

  setAnchor: (day) => set({ anchor: day, selectedEvent: null }),

  selectEvent: (row) =>
    set((state) => ({
      selectedEvent:
        state.selectedEvent !== null && sameOccurrence(state.selectedEvent, row) ? null : row,
    })),

  clearSelection: () => set({ selectedEvent: null }),
}));
