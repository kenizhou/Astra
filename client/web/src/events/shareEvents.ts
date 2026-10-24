// SPDX-License-Identifier: GPL-3.0-only
// The share event wiring: the host emits one event per share hand-off
// (launch files, or a second launch's argv forwarded by the
// single-instance guard), and this mount routes each landing into the
// composer's share open path. Mounted once at the app root beside the
// mailto mount, StrictMode safe: a registration that resolves after its
// effect unmounted unlistens itself instead of leaking a live duplicate
// handler. The payload's path never logs here or anywhere downstream.

import { isTauri } from '@/events/surfaceEvents';
import { onShareOpen, openShareComposer } from '@/features/composer/shareOpen';

/**
 * Mounts the share listener and returns its disposer. Synchronous on
 * purpose: the caller's effect can unmount before the registration
 * resolves, and the disposed flag below handles that race itself.
 */
export function mountShareEvents(): () => void {
  if (!isTauri) {
    return () => undefined;
  }
  let disposed = false;
  let unlisten: (() => void) | null = null;
  onShareOpen((files) => {
    void openShareComposer(files);
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
      console.error('share-events: the listener failed to register', err);
    },
  );
  return () => {
    disposed = true;
    unlisten?.();
    unlisten = null;
  };
}
