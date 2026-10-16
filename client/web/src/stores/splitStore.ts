// SPDX-License-Identifier: GPL-3.0-only
// The split inbox's state: the per-account maps the host serves beside
// every mailbox-list snapshot. The map costs no model call. Since the
// kylins filter tabs replaced the split segment, no surface renders from
// this store today; it stays fed so the split's next consumer (a future
// slice) finds the data where the pulls always left it.

import { create } from 'zustand';

import type { SplitEntry, SplitSide } from '@/services/ai';
import type { MessageRowView } from '@/services/surfaces';

/** Every account's split map: the account's message keys by their side. */
export type SplitEntries = Record<string, Map<string, SplitSide>>;

interface SplitStoreState {
  /** The split maps the last pulls served, keyed by account id. */
  entries: SplitEntries;
  /** Replaces one account's map from a fresh pull. */
  applyMap: (account: string, entries: SplitEntry[]) => void;
}

export const useSplitStore = create<SplitStoreState>((set) => ({
  entries: {},

  applyMap: (account, entries) =>
    set((state) => ({
      entries: {
        ...state.entries,
        [account]: new Map(entries.map((entry) => [entry.key, entry.side])),
      },
    })),
}));

/**
 * The side a row lands on: its account's map's word for the message it
 * shows. A thread row names no single message and an unmapped message
 * has no stored category, so both answer null.
 */
export function sideOf(row: MessageRowView, entries: SplitEntries): SplitSide | null {
  if (!('message' in row.key)) {
    return null;
  }
  return entries[row.account_id]?.get(row.key.message.key) ?? null;
}
