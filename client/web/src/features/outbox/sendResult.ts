// SPDX-License-Identifier: GPL-3.0-only
// The send-result decision core: which queued sends count as delivered. A
// row leaving the queue is a delivery only when this session's own hands did
// not remove it: Cancel withdraws a send and Mark as sent settles one, and
// the panel's own click is feedback enough for those. A removal the host
// refused never reached the queue, so its mark comes back off at once; one
// the engine answered is judged by the snapshot that follows, which takes
// the mark back off a row still standing. Pure helpers over snapshots, so
// the policy stays pinned by tests without a clock.

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
  /** The removals the engine has answered and the next snapshot has not judged yet. */
  answered: string[];
  /** Shows the toast; a showing one restarts its moment. */
  show: (message: string) => void;
  /** Hides the toast. */
  dismiss: () => void;
  /** Records a removal the user made themselves: Cancel, or Mark as sent. */
  noteUserSettled: (queued: QueuedKey) => void;
  /**
   * Takes a recorded removal back: the action never reached the queue, so
   * the send's later genuine delivery still counts.
   */
  unsettle: (queued: QueuedKey) => void;
  /**
   * Records the engine's answer to a removal verb; the snapshot that follows
   * judges whether the action took.
   */
  noteEngineAnswered: (queued: QueuedKey) => void;
  /**
   * Judges the answered removals against one snapshot: a row still standing
   * after the engine's answer was refused by it (the send went in flight
   * under the click), so its mark comes off and the send's later genuine
   * delivery still counts; a row gone lets the mark stand.
   */
  unsettleRefused: (rows: QueuedRow[]) => void;
}

export const useSendResultStore = create<SendResultStoreState>((set) => ({
  toast: null,
  settled: [],
  answered: [],

  show: (message) => set({ toast: message }),

  dismiss: () => set({ toast: null }),

  noteUserSettled: (queued) =>
    set((state) => ({ settled: [...state.settled, keyOf(queued)] })),

  unsettle: (queued) =>
    set((state) => ({ settled: state.settled.filter((key) => key !== keyOf(queued)) })),

  noteEngineAnswered: (queued) =>
    set((state) => ({ answered: [...state.answered, keyOf(queued)] })),

  unsettleRefused: (rows) =>
    set((state) => {
      if (state.answered.length === 0) {
        return state;
      }
      const standing = new Set(rows.map((row) => keyOf(row.queued)));
      const refused = state.answered.filter((key) => standing.has(key));
      if (refused.length === 0) {
        return { answered: [] };
      }
      return {
        answered: [],
        settled: state.settled.filter((key) => !refused.includes(key)),
      };
    }),
}));
