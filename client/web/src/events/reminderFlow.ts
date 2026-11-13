// SPDX-License-Identifier: GPL-3.0-only
// The reminder flow: the subscription the notification history records
// from. The host emits the woken batch on the reminder event after the
// enabled gate (no focus suppression), so what crosses here is exactly the
// reminders that were notified. The payload is the same new-mail batch shape
// the history store already records.

import { listen } from '@tauri-apps/api/event';

import type { NewMailBatchWire } from '@/stores/notificationHistoryStore';

/** The event the host's woken reminder batch crosses on. */
export const REMINDER_EVENT = 'reminder';

/**
 * Subscribes to the host's woken reminder batches and resolves its
 * unlistener; each payload is the batch (the messages plus the overflow
 * count, always zero for reminders).
 */
export function onReminder(handler: (batch: NewMailBatchWire) => void): Promise<() => void> {
  return listen<NewMailBatchWire>(REMINDER_EVENT, (event) => {
    handler(event.payload);
  });
}
