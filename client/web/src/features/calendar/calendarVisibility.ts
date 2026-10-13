// SPDX-License-Identifier: GPL-3.0-only
// The calendar visibility toggle, shared by the rail's checkbox list and the
// toolbar's manager dropdown: the intent names the visible set outright, so
// hiding a calendar drops it from the currently visible ids and showing one
// adds it, never disturbing the visibility of any other row.

import { dispatch, intents } from '@/services/ipc';
import type { CalendarRow } from '@/services/ipc';

/** Names one calendar shown or hidden, dispatching the resulting visible set. */
export function setCalendarVisible(
  calendars: CalendarRow[],
  account: string,
  id: string,
  visible: boolean,
): void {
  const ids = calendars
    .filter((calendar) =>
      calendar.account === account && calendar.id === id ? visible : calendar.visible,
    )
    .map((calendar) => ({ account: calendar.account, key: calendar.id }));
  void dispatch(intents.setVisibleCalendars(ids));
}
