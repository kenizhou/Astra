// SPDX-License-Identifier: GPL-3.0-only
// The offline banner: a slim strip over the shell's top while every dialled
// account stands in outage. Its words name reconnecting, never the word
// error, because the loop is already trying again; it overlays the shell
// rather than joining the flow, so its arrival and its departure shift no
// layout; and it leaves with the first sync-progress pull that reports a
// healthy account.

import { useUiStore } from '@/stores/uiStore';

/** The offline banner over the shell's top edge. */
export function OfflineBanner() {
  const offline = useUiStore((s) => s.delivery.offline_banner);

  if (!offline) {
    return null;
  }

  return (
    <div
      role="status"
      className="absolute top-[var(--header-h)] left-1/2 z-[var(--z-dropdown)] -translate-x-1/2 rounded-[var(--radius)] border border-[var(--warning)]/50 bg-surface px-3 py-1 text-xs text-foreground shadow-lg"
    >
      Offline and reconnecting. New mail arrives as soon as the connection is back.
    </div>
  );
}
