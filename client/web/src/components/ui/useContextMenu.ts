// SPDX-License-Identifier: GPL-3.0-only
// The position-and-dismissal half of the context menu, shared by every
// surface: `open` suppresses the native menu and remembers the cursor,
// `close` drops the state; the surface owns the item list itself.

import { useCallback, useState } from 'react';

export interface ContextMenuState {
  x: number;
  y: number;
}

export function useContextMenu() {
  const [state, setState] = useState<ContextMenuState | null>(null);

  const open = useCallback((event: React.MouseEvent) => {
    event.preventDefault();
    setState({ x: event.clientX, y: event.clientY });
  }, []);

  const close = useCallback(() => setState(null), []);

  return { state, open, close };
}
