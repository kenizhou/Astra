// SPDX-License-Identifier: GPL-3.0-only
// The new-mail flow: the subscription the notification history records
// from. The host emits the notified batch on the new-mail event after the
// enabled/focus gate, so what crosses here is exactly the mail that was
// notified, not every arrival.

import { listen } from '@tauri-apps/api/event';

import type { NewMailBatchWire } from '@/stores/notificationHistoryStore';

/** The event the host's notified batch crosses on. */
export const NEW_MAIL_EVENT = 'new-mail';

/**
 * Subscribes to the host's notified batches and resolves its unlistener;
 * each payload is the drained batch (the messages plus the overflow count).
 */
export function onNewMail(handler: (batch: NewMailBatchWire) => void): Promise<() => void> {
  return listen<NewMailBatchWire>(NEW_MAIL_EVENT, (event) => {
    handler(event.payload);
  });
}