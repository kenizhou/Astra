// SPDX-License-Identifier: GPL-3.0-only
// The mailto event wiring: the host emits one event per parsed mailto
// open (a deep link the OS handed over, or the answer to the reading
// pane's own ask), and this mount routes each landing into the composer's
// mailto open path. Mounted once at the app root, StrictMode safe: a
// registration that resolves after its effect unmounted unlistens itself
// instead of leaking a live duplicate handler, the same dispose race the
// surface, agent, and draft-pool registries handle.

import { isTauri } from '@/events/surfaceEvents';
import { onMailtoOpen, openMailtoComposer } from '@/features/composer/mailtoOpen';

/**
 * Mounts the mailto listener and returns its disposer. Synchronous on
 * purpose: the caller's effect can unmount before the registration
 * resolves, and the disposed flag below handles that race itself.
 */
export function mountMailtoEvents(): () => void {
  if (!isTauri) {
    return () => undefined;
  }
  let disposed = false;
  let unlisten: (() => void) | null = null;
  onMailtoOpen((open) => {
    void openMailtoComposer(open);
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
      console.error('mailto-events: the listener failed to register', err);
    },
  );
  return () => {
    disposed = true;
    unlisten?.();
    unlisten = null;
  };
}
