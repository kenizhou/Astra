// SPDX-License-Identifier: GPL-3.0-only
// The send-result decision core: which queued sends count as delivered. A
// row leaving the queue is a delivery only when this session's own hands did
// not remove it: Cancel withdraws a send and Mark as sent settles one, and
// the panel's own click is feedback enough for those. Pure helpers over
// snapshots, so the policy stays pinned by tests without a clock.

import { create } from 'zustand';

import type { QueuedKey, QueuedRow } from '@/services/ipc';

/** How long a send-result toast stays up. */
export const SEND_TOAST_MS = 6_000;

/** One queued send's stable name, for comparing across snapshots. */
export function keyOf(queued: QueuedKey): string {
  return `${queued.account}/${queued.op}`;
}

/** The sends that left the queue between two snapshots: held before, gone after. */
export function vanishedKeys(before: QueuedRow[], after: QueuedRow[]): string[] {
  const afterKeys = new Set(after.map((row) => keyOf(row.queued)));
  return before.map((row) => keyOf(row.queued)).filter((key) => !afterKeys.has(key));
}

interface SendResultStoreState {
  /** The showing toast's words; null when it is down. */
  toast: string | null;
  /** The keys this session's own removal verbs took out, so their leaving is not read as a delivery. */
  settled: string[];
  /** Shows the toast; a showing one restarts its moment. */
  show: (message: string) => void;
  /** Hides the toast. */
  dismiss: () => void;
  /** Records a removal the user made themselves: Cancel, or Mark as sent. */
  noteUserSettled: (queued: QueuedKey) => void;
}

export const useSendResultStore = create<SendResultStoreState>((set) => ({
  toast: null,
  settled: [],

  show: (message) => set({ toast: message }),

  dismiss: () => set({ toast: null }),

  noteUserSettled: (queued) =>
    set((state) => ({ settled: [...state.settled, keyOf(queued)] })),
}));
