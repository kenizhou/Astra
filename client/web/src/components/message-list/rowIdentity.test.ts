// SPDX-License-Identifier: GPL-3.0-only
// The row identity pins: one stable string per row key, distinct across
// kinds, accounts, and values, so React keys and selection matches never
// collide.
import { describe, expect, it } from 'vitest';

import { rowKeyId, sameRowKey } from './rowIdentity';
import type { RowKey } from '@/services/surfaces';

describe('rowKeyId', () => {
  it('names a message row by its kind, account, and key', () => {
    const key: RowKey = { message: { account: 'work', key: 'imap:v1:u42' } };
    expect(rowKeyId(key)).toBe('message:work:imap:v1:u42');
  });

  it('names a thread row by its kind, account, and thread id', () => {
    const key: RowKey = { thread: { account: 'work', thread_id: 'th-7' } };
    expect(rowKeyId(key)).toBe('thread:work:th-7');
  });

  it('separates rows that differ in any half', () => {
    const ids = [
      rowKeyId({ message: { account: 'work', key: 'k' } }),
      rowKeyId({ message: { account: 'home', key: 'k' } }),
      rowKeyId({ thread: { account: 'work', thread_id: 'k' } }),
    ];
    expect(new Set(ids).size).toBe(3);
  });
});

describe('sameRowKey', () => {
  it('matches structurally equal keys and rejects the rest', () => {
    expect(sameRowKey({ message: { account: 'work', key: 'k' } }, { message: { account: 'work', key: 'k' } })).toBe(
      true,
    );
    expect(sameRowKey({ message: { account: 'work', key: 'k' } }, { thread: { account: 'work', thread_id: 'k' } })).toBe(
      false,
    );
    expect(sameRowKey({ thread: { account: 'home', thread_id: 't' } }, null)).toBe(false);
  });
});
