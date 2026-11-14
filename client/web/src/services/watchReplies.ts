// SPDX-License-Identifier: GPL-3.0-only
// The watch-replies half of the IPC wire: the row menu's Watch replies /
// Unwatch verbs and the watch listing. Split from ipc.ts to keep that file
// under the line cap (the wire types ride here, never there), like the
// category half before it. Watches are host commands, not intents, and a
// LOCAL membership mark: no engine write, no sync, no protocol concept. The
// first reply to a watched conversation raises a "Reply received" notice and
// clears the watch (one-shot); the watch never moves mail.

import { invoke } from '@tauri-apps/api/core';

import type { RowRef } from './ipc';

/** One watch as the host carries it: the account and thread id resolved at
 *  set time (the row's handle), the representative member key the check
 *  re-resolves through the engine, and the creation stamp (unix seconds). */
export interface WatchEntry {
  account: string;
  thread_id: string;
  member_key: string;
  created_at: number;
}

/** Watches the row's conversation, answering whether this call inserted
 *  (idempotent). The core captures one representative member key. */
export function watchSet(row: RowRef): Promise<boolean> {
  return invoke('watch_set', { row });
}

/** Unwatches the row's conversation, answering whether a row was removed
 *  (idempotent). The one-shot consume on the first reply rides the same verb. */
export function watchClear(row: RowRef): Promise<boolean> {
  return invoke('watch_clear', { row });
}

/** Every watch, insertion order; an app with no store answers empty. */
export function watchList(): Promise<WatchEntry[]> {
  return invoke('watch_list');
}