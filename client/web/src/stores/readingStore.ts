// SPDX-License-Identifier: GPL-3.0-only
// The reading pane's state: the last reading snapshot and the reader's
// show-remote-images opt-in. The flag is local to the surface and resets the
// moment the open message changes, so a new message always starts gated
// while a republish of the same message (a refresh) keeps the choice.

import { create } from 'zustand';

import type { ReadingSnapshot } from '@/services/ipc';

/** Whether two snapshots hold the same message; null matches only null. */
function sameMessage(a: ReadingSnapshot | null, b: ReadingSnapshot | null): boolean {
  if (a === null || b === null) {
    return a === b;
  }
  return a.message.account === b.message.account && a.message.key === b.message.key;
}

interface ReadingStoreState {
  /** The open message's snapshot; null while the pane is closed. */
  snapshot: ReadingSnapshot | null;
  /** Whether the reader opted into remote images for the open message. */
  showRemoteImages: boolean;
  /** Applies a pulled snapshot, resetting the opt-in on a message change. */
  applySnapshot: (snapshot: ReadingSnapshot | null) => void;
  /** Records the reader's remote-images opt-in for the open message. */
  setShowRemoteImages: (show: boolean) => void;
}

export const useReadingStore = create<ReadingStoreState>((set) => ({
  snapshot: null,
  showRemoteImages: false,

  applySnapshot: (snapshot) =>
    set((state) => ({
      snapshot,
      showRemoteImages: sameMessage(state.snapshot, snapshot) ? state.showRemoteImages : false,
    })),

  setShowRemoteImages: (show) => set({ showRemoteImages: show }),
}));

/**
 * The open message's attachment part ids, the exact set the reading snapshot
 * lists (the invitation rule already hid the parts it owns): what a forward
 * carries, pre-attached and removable one by one.
 */
export function carriedAttachmentIds(): number[] {
  return (useReadingStore.getState().snapshot?.attachments ?? []).map((row) => row.id);
}
