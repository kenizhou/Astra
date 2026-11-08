// SPDX-License-Identifier: GPL-3.0-only
// The per-account outage badge: a small warning pill on the account's group
// header while the account stands in outage, the same shape the unread
// badge wears with its own glyph, its tooltip naming the account and its
// reconnect. It leaves with the first healthy pass the sync-progress
// snapshot reports.

import { useUiStore } from '@/stores/uiStore';

/** The outage badge on one account's group header. */
export function OutageBadge({ accountId, displayName }: { accountId: string; displayName: string }) {
  const outages = useUiStore((s) => s.delivery.outages);

  if (outages?.includes(accountId) !== true) {
    return null;
  }

  return (
    <span
      data-outage-badge
      title={`${displayName} is offline and reconnecting`}
      className="shrink-0 rounded-full bg-[var(--warning)] px-1.5 py-0.5 text-[11px] font-medium leading-none text-[var(--warning-foreground)]"
    >
      !
    </span>
  );
}
