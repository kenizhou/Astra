// SPDX-License-Identifier: GPL-3.0-only
// The snooze half of the IPC wire: the command that sets a message aside
// until a wake instant, and the picker's computed wake choices. Split from
// ipc.ts to keep that file under the line cap (the wire types ride here,
// never there), like the report and write halves before it. Snooze is a host
// command, not an intent: the core records the stable Message-ID and moves
// the message into the account's Snooze folder, the wake timer returns it.

import { invoke } from '@tauri-apps/api/core';

import type { MessageRef } from './ipc';

/** One wake choice the picker offers: a label and its instant, unix seconds. */
export interface SnoozeChoice {
  /** The choice's label, as the picker renders it. */
  label: string;
  /** The wake instant, unix seconds. */
  wakeAt: number;
}

/** Sets one message aside until the wake instant (unix seconds): the core
 *  records the stable identity and moves it into the Snooze folder. */
export function snoozeMessage(message: MessageRef, wakeAt: number): Promise<void> {
  return invoke('snooze_message', {
    account: message.account,
    key: message.key,
    wakeAt,
  });
}

/** The picker's three wake choices as computed instants over `now`: later
 *  today (this evening), tomorrow morning, and next week. Pure conversions,
 *  so the picker and its tests read the same instants. */
export function snoozeChoices(now: Date): SnoozeChoice[] {
  const laterToday = new Date(now);
  laterToday.setHours(18, 0, 0, 0);
  const tomorrow = new Date(now);
  tomorrow.setDate(tomorrow.getDate() + 1);
  tomorrow.setHours(8, 0, 0, 0);
  const nextWeek = new Date(now);
  nextWeek.setDate(nextWeek.getDate() + 7);
  nextWeek.setHours(8, 0, 0, 0);
  return [
    { label: 'Later today', wakeAt: unixSeconds(laterToday) },
    { label: 'Tomorrow', wakeAt: unixSeconds(tomorrow) },
    { label: 'Next week', wakeAt: unixSeconds(nextWeek) },
  ];
}

/** A local instant as whole unix seconds. */
function unixSeconds(instant: Date): number {
  return Math.floor(instant.getTime() / 1000);
}