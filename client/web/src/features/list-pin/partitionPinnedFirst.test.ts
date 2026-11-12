// SPDX-License-Identifier: GPL-3.0-only
// The pin partition: pinned rows float first, the engine order holds within
// each partition (no date re-sort), the input never mutates, and an empty
// pin set is the identity.
import { describe, expect, it } from 'vitest';

import { partitionPinnedFirst } from './partitionPinnedFirst';
import { rowKeyId } from '@/components/message-list/rowIdentity';
import type { MessageRowView } from '@/services/surfaces';

/** One flat row on its own key; the sent_at runs against the engine order
 *  so a date re-sort would show. */
function row(key: string, sentAt: string): MessageRowView {
  return {
    key: { message: { account: 'work', key } },
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: `Subject ${key}`,
    sent_at: sentAt,
    unread: false,
    flagged: false,
    has_attachments: false,
  };
}

const r1 = row('k1', '2026-10-05T08:00:00Z');
const r2 = row('k2', '2026-10-06T08:00:00Z');
const r3 = row('k3', '2026-10-07T08:00:00Z');
const r4 = row('k4', '2026-10-08T08:00:00Z');
const rows = [r1, r2, r3, r4];

const idOf = (r: MessageRowView): string => rowKeyId(r.key);

describe('partitionPinnedFirst', () => {
  it('floats the pinned rows to the top', () => {
    const pinned = new Set([idOf(r3)]);
    expect(partitionPinnedFirst(rows, pinned)).toEqual([r3, r1, r2, r4]);
  });

  it('preserves the engine order within each partition, not the date order', () => {
    const pinned = new Set([idOf(r4), idOf(r2)]);
    // r2 before r4 in the pinned half (engine order, though r4 is newer);
    // r1 before r3 in the rest.
    expect(partitionPinnedFirst(rows, pinned)).toEqual([r2, r4, r1, r3]);
  });

  it('does not mutate the input', () => {
    const input = [r1, r2, r3, r4];
    const pinned = new Set([idOf(r3), idOf(r1)]);
    const result = partitionPinnedFirst(input, pinned);
    expect(input).toEqual([r1, r2, r3, r4]);
    expect(result).not.toBe(input);
    expect(result).toEqual([r1, r3, r2, r4]);
  });

  it('is the identity on an empty pin set', () => {
    expect(partitionPinnedFirst(rows, new Set())).toBe(rows);
  });

  it('keeps the order when no row matches the pin set', () => {
    const pinned = new Set(['message:work:absent']);
    expect(partitionPinnedFirst(rows, pinned)).toEqual(rows);
  });

  it('pins thread rows by their own identity', () => {
    const thread: MessageRowView = {
      ...r3,
      key: { thread: { account: 'work', thread_id: 'th-9' } },
    };
    const mixed = [r1, thread, r2];
    const pinned = new Set([rowKeyId(thread.key)]);
    expect(partitionPinnedFirst(mixed, pinned)).toEqual([thread, r1, r2]);
  });

  it('returns every row exactly once', () => {
    const pinned = new Set([idOf(r2), idOf(r4)]);
    const result = partitionPinnedFirst(rows, pinned);
    expect(result).toHaveLength(rows.length);
    expect(new Set(result.map(idOf))).toEqual(new Set(rows.map(idOf)));
  });
});
