// SPDX-License-Identifier: GPL-3.0-only
// The swipe toast: the sentence naming the act that fired, its Undo where a
// reverse is expressible, and the quiet dismissal once the moment passes.

import { useEffect } from 'react';

import { dispatch } from '@/services/ipc';

import { useSwipeToastStore } from './swipeToastStore';

/** How long the undo stands before the toast dismisses itself. */
export const SWIPE_TOAST_MS = 6000;

export function SwipeToast() {
  const note = useSwipeToastStore((s) => s.note);
  const undo = useSwipeToastStore((s) => s.undo);

  useEffect(() => {
    if (note === null) {
      return;
    }
    const timer = setTimeout(() => useSwipeToastStore.getState().dismiss(), SWIPE_TOAST_MS);
    return () => clearTimeout(timer);
  }, [note]);

  if (note === null) {
    return null;
  }

  return (
    <div
      role="status"
      className="absolute bottom-3 left-3 z-[var(--z-toast)] flex items-center gap-3 rounded-[var(--radius)] border border-border bg-surface px-3 py-2 text-xs text-foreground shadow-lg"
    >
      <span>{note}</span>
      {undo !== null && (
        <button
          type="button"
          onClick={() => {
            void dispatch(undo);
            useSwipeToastStore.getState().dismiss();
          }}
          className="shrink-0 rounded-[var(--radius-xs)] px-2 py-1 font-medium text-primary transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          Undo
        </button>
      )}
    </div>
  );
}
