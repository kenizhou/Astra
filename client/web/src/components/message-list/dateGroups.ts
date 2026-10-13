// SPDX-License-Identifier: GPL-3.0-only
// The message list's item model: what one virtualised row draws, a date group
// header or a message row. The folder listing walks its rows newest first and
// opens a header whenever the sent-at day's label changes, the way KY's list
// buckets its threads; the search results stay flat.

import type { MessageRowView } from '@/services/surfaces';
import { sentDayLabel } from './sentAt';

/** One virtualised list row: a group header, or a message. */
export type ListItem =
  | { kind: 'header'; label: string }
  | { kind: 'row'; row: MessageRowView };

/** A header row's height in pixels; the virtualiser's estimate for one. */
export const HEADER_ROW_PX = 30;

/** Wraps rows without headers; the search results mode's flat list. */
export function flatItems(rows: MessageRowView[]): ListItem[] {
  return rows.map((row) => ({ kind: 'row', row }));
}

/** Interleaves the folder rows with their date group headers, one header per
 * run of rows sharing a sent-at day. */
export function withDateHeaders(rows: MessageRowView[], now: Date): ListItem[] {
  const items: ListItem[] = [];
  let lastLabel = '';
  for (const row of rows) {
    const label = sentDayLabel(row.sent_at, now);
    if (label !== lastLabel) {
      items.push({ kind: 'header', label });
      lastLabel = label;
    }
    items.push({ kind: 'row', row });
  }
  return items;
}
