// SPDX-License-Identifier: GPL-3.0-only
// The watch-replies set: the conversations the user watches for a first
// reply, seeded from the host's watch listing and refreshed on every toggle.
// Pure client-side view state over a local membership mark (the category
// store's shape): the row menu reads it to flip Watch replies / Unwatch. The
// set keys by account and id, holding both each watch's thread id and its
// representative member key, so a thread row resolves by its thread id and a
// message row by the member key the core captured at set time. The watch
// never moves mail; the first reply clears it core-side and the next refresh
// drops it here.

import { create } from 'zustand';

import type { MessageRowView } from '@/services/surfaces';
import { watchClear, watchList, watchSet, type WatchEntry } from '@/services/watchReplies';

interface WatchStoreState {
  /** The watched identities, as `account:id` strings (thread ids and member
   *  keys alike); empty until the first load lands. */
  watched: ReadonlySet<string>;
  /** Whether the first load has landed (the loader runs once). */
  loaded: boolean;
  /** Bumped on every load: one snapshot's distinguisher from the next. */
  revision: number;
  /** Pulls the watches from the host and bumps the revision. */
  loadWatches: () => Promise<void>;
  /** Whether the row's conversation stands watched. */
  isWatched: (row: MessageRowView) => boolean;
  /** Watches the row's conversation, then refreshes the set. */
  watch: (row: MessageRowView) => Promise<void>;
  /** Unwatches the row's conversation, then refreshes the set. */
  unwatch: (row: MessageRowView) => Promise<void>;
}

/** The watched identity a row resolves to: a message row by its provider key
 *  (the member key the core captures), a thread row by its thread id. */
function rowWatchId(row: MessageRowView): string {
  return 'message' in row.key
    ? `${row.key.message.account}:${row.key.message.key}`
    : `${row.key.thread.account}:${row.key.thread.thread_id}`;
}

/** Seeds the set from the listing: each watch contributes its thread id and
 *  its representative member key, so both row kinds resolve. */
function seed(entries: WatchEntry[]): Set<string> {
  const watched = new Set<string>();
  for (const entry of entries) {
    watched.add(`${entry.account}:${entry.thread_id}`);
    watched.add(`${entry.account}:${entry.member_key}`);
  }
  return watched;
}

export const useWatchStore = create<WatchStoreState>((set, get) => ({
  watched: new Set(),
  loaded: false,
  revision: 0,
  loadWatches: async () => {
    try {
      const entries = await watchList();
      set((state) => ({ watched: seed(entries), loaded: true, revision: state.revision + 1 }));
    } catch {
      // A failed pull leaves the set empty: the menu still renders, the
      // label simply reads unwatched until a later load lands.
      set({ loaded: true });
    }
  },
  isWatched: (row) => get().watched.has(rowWatchId(row)),
  watch: async (row) => {
    await watchSet(row.key);
    await get().loadWatches();
  },
  unwatch: async (row) => {
    await watchClear(row.key);
    await get().loadWatches();
  },
}));