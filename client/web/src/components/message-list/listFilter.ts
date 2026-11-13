// SPDX-License-Identifier: GPL-3.0-only
// The list filter: which rows the message list shows, client-side only, so
// it never crosses an intent. Ported from kylins' quick-filter tabs (All /
// Unread / Flagged / Attachments / VIP); the predicates read flags the wire
// already carries on every row, thread rows included, and the VIP tab
// reads the sender set the caller passes in (the pin pattern: the set is
// read at the list's render choke point, the predicate stays pure).

import type { MessageRowView } from '@/services/surfaces';
import { normaliseAddress } from '@/features/vip/vipSendersStore';

/** The tabs the toolbar offers. */
export type ListFilter = 'all' | 'unread' | 'flagged' | 'attachments' | 'vip';

/** The empty VIP set the callers that hold none pass by default. */
const NO_VIP: ReadonlySet<string> = new Set();

/** The rows one tab shows, in the snapshot's own order. The VIP set holds
 *  normalised bare addresses; an absent set answers no VIP rows. */
export function filteredRows(
  rows: MessageRowView[],
  filter: ListFilter,
  vip: ReadonlySet<string> = NO_VIP,
): MessageRowView[] {
  switch (filter) {
    case 'all':
      return rows;
    case 'unread':
      return rows.filter((row) => row.unread);
    case 'flagged':
      return rows.filter((row) => row.flagged);
    case 'attachments':
      return rows.filter((row) => row.has_attachments);
    case 'vip':
      return rows.filter((row) => vip.has(normaliseAddress(row.sender_address)));
  }
}

/** The count badges the toolbar draws, over the unfiltered rows. */
export function filterCounts(
  rows: MessageRowView[],
  vip: ReadonlySet<string> = NO_VIP,
): Record<ListFilter, number> {
  let unread = 0;
  let flagged = 0;
  let attachments = 0;
  let vipCount = 0;
  for (const row of rows) {
    if (row.unread) {
      unread += 1;
    }
    if (row.flagged) {
      flagged += 1;
    }
    if (row.has_attachments) {
      attachments += 1;
    }
    if (vip.has(normaliseAddress(row.sender_address))) {
      vipCount += 1;
    }
  }
  return { all: rows.length, unread, flagged, attachments, vip: vipCount };
}