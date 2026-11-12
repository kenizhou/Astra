// SPDX-License-Identifier: GPL-3.0-only
// The pin overlay's ordering: pinned rows float to the top, each partition
// keeping the engine order it arrived in. No date re-sort: the engine's
// order is total with tie-breaks, and the overlay only partitions. Pure and
// non-mutating; an empty pin set is the identity.

import { rowKeyId } from '@/components/message-list/rowIdentity';
import type { MessageRowView } from '@/services/surfaces';

/** Pinned rows first, the engine order preserved within each partition. */
export function partitionPinnedFirst(
  rows: MessageRowView[],
  pinned: ReadonlySet<string>,
): MessageRowView[] {
  if (pinned.size === 0) {
    return rows;
  }
  const first: MessageRowView[] = [];
  const rest: MessageRowView[] = [];
  for (const row of rows) {
    if (pinned.has(rowKeyId(row.key))) {
      first.push(row);
    } else {
      rest.push(row);
    }
  }
  return [...first, ...rest];
}
