// SPDX-License-Identifier: GPL-3.0-only
// The ticking now: the snapshot's now data places the line at publish, and
// the host clock advances it between publishes, so the line never stands
// still while the view is open even when nothing republishes.

import { useEffect, useState } from 'react';

/** How often the host clock is re-read, in milliseconds. */
export const NOW_TICK_MS = 30_000;

/**
 * The current instant as the host clock reads it, refreshed on a tick. Null
 * while no zone has been stated (nothing to localise against).
 */
export function useZonedNow(zone: string | null): Date | null {
  const [now, setNow] = useState<Date>(() => new Date());

  useEffect(() => {
    if (zone === null) {
      return;
    }
    setNow(new Date());
    const timer = setInterval(() => setNow(new Date()), NOW_TICK_MS);
    return () => clearInterval(timer);
  }, [zone]);

  return zone === null ? null : now;
}
