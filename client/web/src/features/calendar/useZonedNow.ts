// SPDX-License-Identifier: GPL-3.0-only
// The ticking now: drawn from the snapshot's own now data and advanced by the
// host clock between publishes. The snapshot's instant anchors the line and
// only the host time elapsed since the read moves it, so a host clock running
// ahead or behind never displaces the line by its skew, and the line never
// stands still while the view is open even when nothing republishes.

import { useEffect, useState } from 'react';

import type { NowLineData } from '@/services/ipc';

/** How often the host clock is re-read, in milliseconds. */
export const NOW_TICK_MS = 30_000;

/**
 * The instant the now line draws at: the snapshot's `now` advanced by the
 * host time elapsed since the anchor was taken. Null while no snapshot has
 * carried now data (nothing to localise against).
 */
export function useZonedNow(nowData: NowLineData | null): Date | null {
  const published = nowData?.now ?? null;
  const [now, setNow] = useState<Date | null>(null);

  useEffect(() => {
    if (published === null) {
      return;
    }
    const anchor = new Date(published).getTime();
    const hostAtAnchor = Date.now();
    const read = (): void => setNow(new Date(anchor + (Date.now() - hostAtAnchor)));
    read();
    const timer = setInterval(read, NOW_TICK_MS);
    return () => clearInterval(timer);
  }, [published]);

  return published === null ? null : now;
}
