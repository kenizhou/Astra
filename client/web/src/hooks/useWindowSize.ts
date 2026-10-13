// SPDX-License-Identifier: GPL-3.0-only
// The shell's compact breakpoint: under it the folder pane leaves the panel
// row and rides the drawer the tool rail opens.

import { useEffect, useState } from 'react';

/** The window width under which the shell goes compact. */
export const COMPACT_MAX_WIDTH = 900;

/** Whether the window is under the shell's compact breakpoint. */
export function useIsCompact(): boolean {
  const [compact, setCompact] = useState(
    () => typeof window !== 'undefined' && window.innerWidth < COMPACT_MAX_WIDTH,
  );

  useEffect(() => {
    const onResize = () => setCompact(window.innerWidth < COMPACT_MAX_WIDTH);
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  return compact;
}
