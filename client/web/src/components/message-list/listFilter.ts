// SPDX-License-Identifier: GPL-3.0-only
// The list filter: which rows the message list shows, client-side only, so
// it never crosses an intent. Ported from kylins' quick-filter tabs (All /
// Unread / Flagged / Attachments); the predicates read flags the wire
// already carries on every row, thread rows included.

import type { MessageRowView } from '@/services/surfaces';

/** The tabs the toolbar offers. */
export type ListFilter = 'all' | 'unread' | 'flagged' | 'attachments';

/** The rows one tab shows, in the snapshot's own order. */
export function filteredRows(rows: MessageRowView[], filter: ListFilter): MessageRowView[] {
  switch (filter) {
    case 'all':
      return rows;
    case 'unread':
      return rows.filter((row) => row.unread);
    case 'flagged':
      return rows.filter((row) => row.flagged);
    case 'attachments':
      return rows.filter((row) => row.has_attachments);
  }
}

/** The count badges the toolbar draws, over the unfiltered rows. */
export function filterCounts(rows: MessageRowView[]): Record<ListFilter, number> {
  let unread = 0;
  let flagged = 0;
  let attachments = 0;
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
  }
  return { all: rows.length, unread, flagged, attachments };
}
