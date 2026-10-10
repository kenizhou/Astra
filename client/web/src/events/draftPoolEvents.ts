// SPDX-License-Identifier: GPL-3.0-only
// The draft-pool event wiring: the host emits one event per landed pool row
// (drafted or skipped alike), and this mount folds each landing into the ai
// chips store, which re-pulls only when the landed row is the pane's own
// open message. Mounted once at the app root, StrictMode safe: a
// registration that resolves after its effect unmounted unlistens itself
// instead of leaking a live duplicate handler, the same dispose race the
// surface and agent registries handle.

import { isTauri } from '@/events/surfaceEvents';
import { onDraftPoolLanded } from '@/services/ai';
import { useAiChipsStore } from '@/stores/aiChipsStore';

/**
 * Mounts the draft-pool listener and returns its disposer. Synchronous on
 * purpose: the caller's effect can unmount before the registration resolves,
 * and the disposed flag below handles that race itself.
 */
export function mountDraftPoolEvents(): () => void {
  if (!isTauri) {
    return () => undefined;
  }
  let disposed = false;
  let unlisten: (() => void) | null = null;
  onDraftPoolLanded((message) => {
    useAiChipsStore.getState().poolLanded(message);
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
      console.error('draft-pool-events: the listener failed to register', err);
    },
  );
  return () => {
    disposed = true;
    unlisten?.();
    unlisten = null;
  };
}
