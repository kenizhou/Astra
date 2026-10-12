// SPDX-License-Identifier: GPL-3.0-only
// The navigation arms: the anchor date and the loadable window each view
// needs, derived in the snapshot's own zone and dispatched together so the
// core's window and its anchor never disagree. The week-shaped views open
// on the first day the settings block names.

import { dispatch, intents } from '@/services/ipc';

import { instantOfZoned, monthGrid, weekStartOfKey } from './calendarRange';
import type { CalendarView } from './calendarStore';

/**
 * The window a view needs over `anchor`: where it opens and how long it is.
 * `firstDay` is a JS weekday index, the day the week opens on.
 */
export function windowOf(
  view: CalendarView,
  anchor: string,
  zone: string,
  firstDay: number = 1,
): { start: string; days: number } {
  switch (view) {
    case 'day':
      return { start: instantOfZoned(anchor, 0, zone), days: 1 };
    case 'work_week':
      return { start: instantOfZoned(weekStartOfKey(anchor, firstDay), 0, zone), days: 7 };
    case 'agenda':
      return { start: instantOfZoned(anchor, 0, zone), days: 31 };
    case 'month':
      return { start: instantOfZoned(monthGrid(anchor, firstDay)[0]!, 0, zone), days: 42 };
  }
}

/**
 * Moves the grids: the anchor date the surfaces navigate by, then the window
 * the new anchor and view need. Both intents together, because either alone
 * leaves the core holding a window the view is not looking at.
 */
export function navigateTo(
  anchor: string,
  view: CalendarView,
  zone: string,
  firstDay: number = 1,
): void {
  void dispatch(intents.setCalendarDate(anchor));
  const { start, days } = windowOf(view, anchor, zone, firstDay);
  void dispatch(intents.setCalendarRange(start, days));
}
