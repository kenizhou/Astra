// SPDX-License-Identifier: GPL-3.0-only
// The calendar surface's state: the last pulled snapshot's row sets (the
// calendar manager, the joined occurrences, the write states, the reply
// prompts, the now data), the view kind the toolbar switches, and the
// navigation anchor. The window itself lives in the core: the anchor seeds
// once from the snapshot's own now data so a cold calendar opens on today in
// the display zone, and after that only navigation moves it.

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
  /** Applies a pulled snapshot. */
  applySnapshot: (snapshot: CalendarSnapshot) => void;
  /** Switches the view kind. */
  setView: (view: CalendarView) => void;
  /** Moves the navigation anchor to a civil day key. */
  setAnchor: (day: string) => void;
}

export const useCalendarStore = create<CalendarStoreState>((set) => ({
  calendars: [],
  occurrences: [],
  writeStates: [],
  replyPrompts: [],
  nowData: null,
  view: 'month',
  anchor: null,

  applySnapshot: (snapshot) =>
    set((state) => ({
      calendars: snapshot.calendars ?? [],
      occurrences: snapshot.occurrences ?? [],
      writeStates: snapshot.write_states ?? [],
      replyPrompts: snapshot.reply_prompts ?? [],
      nowData: snapshot.now_data ?? null,
      // The anchor is the user's place in the diary, not the clock's: seeded
      // once from the first now data, moved only by navigation after that.
      anchor:
        state.anchor ?? (snapshot.now_data ? zoneToday(snapshot.now_data.now, snapshot.now_data.zone) : null),
    })),

  setView: (view) => set({ view }),

  setAnchor: (day) => set({ anchor: day }),
}));
