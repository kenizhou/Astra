// SPDX-License-Identifier: GPL-3.0-only
// The split inbox's state: the per-account maps the host serves beside
// every mailbox-list snapshot and the side the message list is showing.
// Both are pure client-side views over data the core owns: the map costs
// no model call, and the filter never crosses an intent, exactly like
// the row selection it sits beside.

import { create } from 'zustand';

import type { SplitEntry, SplitSide } from '@/services/ai';
import type { MessageRowView } from '@/services/surfaces';

/** The sides the toolbar offers: everything, or one half of the split. */
export type SplitFilter = 'all' | SplitSide;

/** Every account's split map: the account's message keys by their side. */
export type SplitEntries = Record<string, Map<string, SplitSide>>;

interface SplitStoreState {
  /** The split maps the last pulls served, keyed by account id. */
  entries: SplitEntries;
  /** The side the list is showing; All until a toggle names one. */
  filter: SplitFilter;
  /** Replaces one account's map from a fresh pull. */
  applyMap: (account: string, entries: SplitEntry[]) => void;
  /** Records the side the toolbar chose. */
  setFilter: (filter: SplitFilter) => void;
}

export const useSplitStore = create<SplitStoreState>((set) => ({
  entries: {},
  filter: 'all',

  applyMap: (account, entries) =>
    set((state) => ({
      entries: {
        ...state.entries,
        [account]: new Map(entries.map((entry) => [entry.key, entry.side])),
      },
    })),

  setFilter: (filter) => set({ filter }),
}));

/**
 * The side a row lands on: its account's map's word for the message it
 * shows. A thread row names no single message and an unmapped message
 * has no stored category, so both answer null and ride All only.
 */
export function sideOf(row: MessageRowView, entries: SplitEntries): SplitSide | null {
  if (!('message' in row.key)) {
    return null;
  }
  return entries[row.account_id]?.get(row.key.message.key) ?? null;
}

/** The rows the chosen side shows, in the snapshot's own order. */
export function visibleRows(
  rows: MessageRowView[],
  filter: SplitFilter,
  entries: SplitEntries,
): MessageRowView[] {
  if (filter === 'all') {
    return rows;
  }
  return rows.filter((row) => sideOf(row, entries) === filter);
}

/** The count badges the toolbar draws: the whole list and each half. */
export function splitCounts(
  rows: MessageRowView[],
  entries: SplitEntries,
): { all: number; actioned: number; fyi: number } {
  let actioned = 0;
  let fyi = 0;
  for (const row of rows) {
    const side = sideOf(row, entries);
    if (side === 'actioned') {
      actioned += 1;
    } else if (side === 'fyi') {
      fyi += 1;
    }
  }
  return { all: rows.length, actioned, fyi };
}
