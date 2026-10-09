// SPDX-License-Identifier: GPL-3.0-only
// Row identity helpers: a RowKey is an object on the wire, so the surface
// derives one stable string per key for React keys and compares keys
// structurally for selection.

import type { RowKey } from '@/services/surfaces';

/** One stable string per row key: kind, account, then the value half. */
export function rowKeyId(key: RowKey): string {
  return 'message' in key
    ? `message:${key.message.account}:${key.message.key}`
    : `thread:${key.thread.account}:${key.thread.thread_id}`;
}

/** Whether two row keys name the same row; a null half never matches. */
export function sameRowKey(a: RowKey, b: RowKey | null): boolean {
  if (b === null) {
    return false;
  }
  if ('message' in a) {
    return 'message' in b && a.message.account === b.message.account && a.message.key === b.message.key;
  }
  return 'thread' in b && a.thread.account === b.thread.account && a.thread.thread_id === b.thread.thread_id;
}
