// SPDX-License-Identifier: GPL-3.0-only
// The new-mail event wiring: the host emits one event per notified batch
// (after the enabled/focus gate), and this mount records each landing into
// the notification history. Mounted once at the app root, StrictMode safe:
// a registration that resolves after its effect unmounted unlistens itself
// instead of leaking a live duplicate handler, the mailto mount's sibling.

import { isTauri } from '@/events/surfaceEvents';
import { useNotificationHistoryStore } from '@/stores/notificationHistoryStore';

import { onNewMail } from './newMailFlow';

/**
 * Mounts the new-mail listener and returns its disposer. Synchronous on
 * purpose: the caller's effect can unmount before the registration
 * resolves, and the disposed flag below handles that race itself.
 */
export function mountNewMailEvents(): () => void {
  if (!isTauri) {
    return () => undefined;
  }
  let disposed = false;
  let unlisten: (() => void) | null = null;
  onNewMail((batch) => {
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
      console.error('new-mail-events: the listener failed to register', err);
    },
  );
  return () => {
    disposed = true;
    unlisten?.();
    unlisten = null;
  };
}