// SPDX-License-Identifier: GPL-3.0-only
// The reminder event wiring: the host emits one event per woken reminder
// batch (after the enabled gate, no focus suppression), and this mount
// records each landing into the same notification history the new-mail
// mount feeds. Mounted once at the app root, StrictMode safe: a registration
// that resolves after its effect unmounted unlistens itself instead of
// leaking a live duplicate handler, the new-mail mount's sibling.

import { isTauri } from '@/events/surfaceEvents';
import { useNotificationHistoryStore } from '@/stores/notificationHistoryStore';

import { onReminder } from './reminderFlow';

/**
 * Mounts the reminder listener and returns its disposer. Synchronous on
 * purpose: the caller's effect can unmount before the registration
 * resolves, and the disposed flag below handles that race itself.
 */
export function mountReminderEvents(): () => void {
  if (!isTauri) {
    return () => undefined;
  }
  let disposed = false;
  let unlisten: (() => void) | null = null;
  onReminder((batch) => {
    useNotificationHistoryStore.getState().record(batch);
  }).then(
    (teardown) => {
      if (disposed) {
        // The effect unmounted while the subscription was in flight; tear
        // the late registration down immediately.
        teardown();
      } else {
        unlisten = teardown;
      }
    },
    (err) => {
      console.error('reminder-events: the listener failed to register', err);
    },
  );
  return () => {
    disposed = true;
    unlisten?.();
    unlisten = null;
  };
}
