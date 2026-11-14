// SPDX-License-Identifier: GPL-3.0-only
// The reply flow: the subscription the notification history records from.
// The host emits the watched reply batch on the reply event after the
// enabled gate (no focus suppression), so what crosses here is exactly the
// replies that were notified. The payload is the same new-mail batch shape
// the history store already records.

import { listen } from '@tauri-apps/api/event';

import type { NewMailBatchWire } from '@/stores/notificationHistoryStore';

/** The event the host's watched reply batch crosses on. */
export const REPLY_EVENT = 'reply';

/**
 * Subscribes to the host's watched reply batches and resolves its
 * unlistener; each payload is the batch (the messages plus the overflow
 * count, always zero for replies).
 */
export function onReply(handler: (batch: NewMailBatchWire) => void): Promise<() => void> {
  return listen<NewMailBatchWire>(REPLY_EVENT, (event) => {
    handler(event.payload);
  });
}