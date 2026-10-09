// SPDX-License-Identifier: GPL-3.0-only
// The idle autosave hook: thirty seconds of quiet after the last edit is
// when the composer hands its content to the core, mirroring the core's own
// idle constant. An edit inside the window restarts the clock, an unchanged
// composer costs no dispatch, and the dirtiness is read at fire time so a
// save that landed meanwhile still counts.

import { useEffect, useRef } from 'react';

/** How long a composer sits untouched before its draft is handed over. */
export const DRAFT_IDLE_MS = 30_000;

/** What an idle autosave needs to decide and to do. */
export interface AutosaveOptions {
  /** Whether content changed since the last handover, read when the idle ends. */
  isDirty: () => boolean;
  /** The handover itself: the save dispatch. */
  onIdle: () => void;
  /** The idle span, seamed for tests; the product value by default. */
  idleMs?: number;
}

/** The handle an open composer holds. */
export interface AutosaveHandle {
  /** Records one edit: the quiet interval restarts here. */
  noteEdit: () => void;
}

/**
 * Arms the idle autosave for one composer. Every edit calls `noteEdit`;
 * when the idle passes with changes waiting, `onIdle` runs once and the
 * clock stays stopped until the next edit.
 */
export function useAutosave({ isDirty, onIdle, idleMs = DRAFT_IDLE_MS }: AutosaveOptions): AutosaveHandle {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isDirtyRef = useRef(isDirty);
  const onIdleRef = useRef(onIdle);
  isDirtyRef.current = isDirty;
  onIdleRef.current = onIdle;

  const clear = (): void => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  };

  useEffect(() => clear, []);

  const noteEdit = (): void => {
    clear();
    timer.current = setTimeout(() => {
      timer.current = null;
      if (isDirtyRef.current()) {
        onIdleRef.current();
      }
    }, idleMs);
  };

  // One stable handle for the subscriber that owns it, so an effect keyed
  // on the handle never re-subscribes.
  const handle = useRef<AutosaveHandle>({ noteEdit });
  handle.current.noteEdit = noteEdit;
  return handle.current;
}
