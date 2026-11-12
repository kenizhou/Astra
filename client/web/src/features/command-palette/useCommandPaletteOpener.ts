// SPDX-License-Identifier: GPL-3.0-only
// The palette's keyboard opener: Ctrl+Shift+P (Cmd+Shift+P on the mac
// modifier) toggles the palette from anywhere. Ctrl+K stays the search
// island's; a handler that claimed the key first (a preventDefault upstream)
// keeps it, so the opener never steals a binding another surface owns.

import { useEffect } from 'react';

import { toggleCommandPalette } from './useCommandPalette';

/** Arms the document keydown that toggles the palette; the shell mounts it once. */
export function useCommandPaletteOpener(): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (!(event.ctrlKey || event.metaKey) || !event.shiftKey) {
        return;
      }
      if (event.key.toLowerCase() !== 'p') {
        return;
      }
      if (event.defaultPrevented) {
        return;
      }
      event.preventDefault();
      toggleCommandPalette();
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, []);
}
