// SPDX-License-Identifier: GPL-3.0-only
// The shortcuts help's keyboard opener: a bare ? toggles the overlay from
// anywhere outside a text field. On standard layouts ? is Shift+/, so the
// match is on the logical key alone with no shift test; ctrl, meta, and alt
// keep their own bindings. A handler that claimed the key first keeps it,
// held repeats never re-toggle, and an editable target keeps ? a plain
// character.

import { useEffect } from 'react';

import { toggleShortcutsHelp } from './useShortcutsHelp';

/** True while the event's target sits inside an editable surface. */
function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }
  return target.closest('input, textarea, [contenteditable="true"]') !== null;
}

/** Arms the document keydown that toggles the help; the shell mounts it once. */
export function useShortcutsHelpOpener(): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key !== '?' || event.ctrlKey || event.metaKey || event.altKey) {
        return;
      }
      if (event.defaultPrevented || event.repeat) {
        return;
      }
      if (isEditableTarget(event.target)) {
        return;
      }
      event.preventDefault();
      toggleShortcutsHelp();
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, []);
}