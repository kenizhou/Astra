// SPDX-License-Identifier: GPL-3.0-only
// The shell's window stages: wide shows the folder pane beside the list and
// the reading pane; medium sends the folder pane into the rail's drawer;
// narrow also drops the list while a message is open, so the reading pane
// fills the frame (the pane's close returns to the list). Panels hide from
// the left, so what remains always reads correctly.

import { useEffect, useState } from 'react';

/** The width at and above which every pane stands. */
export const WIDE_MIN_WIDTH = 1080;
/** The width under which only one pane shows at a time. */
export const NARROW_MAX_WIDTH = 720;

export type WindowStage = 'wide' | 'medium' | 'narrow';

function stageOf(width: number): WindowStage {
  if (width >= WIDE_MIN_WIDTH) {
    return 'wide';
  }
  return width < NARROW_MAX_WIDTH ? 'narrow' : 'medium';
}

/** The shell's window stage, re-read on every resize. */
export function useWindowStage(): WindowStage {
  const [stage, setStage] = useState(() =>
    typeof window === 'undefined' ? 'wide' : stageOf(window.innerWidth),
  );

  useEffect(() => {
    const onResize = () => setStage(stageOf(window.innerWidth));
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  return stage;
}
