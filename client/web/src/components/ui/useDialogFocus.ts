// SPDX-License-Identifier: GPL-3.0-only
// The modal focus discipline every dialog shares: the focus moves into the
// dialog on open and returns to what opened it on close. `when` gates the
// move for the always-mounted dialogs that render null while closed.

import { useEffect, useRef, type RefObject } from 'react';

/** Focuses the referenced element while `when` holds, restoring on exit. */
export function useDialogFocus<T extends HTMLElement>(when = true): RefObject<T | null> {
  const ref = useRef<T>(null);
  useEffect(() => {
    if (!when) {
      return;
    }
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    ref.current?.focus();
    return () => previous?.focus();
  }, [when]);
  return ref;
}
