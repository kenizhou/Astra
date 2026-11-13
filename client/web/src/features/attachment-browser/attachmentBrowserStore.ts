// SPDX-License-Identifier: GPL-3.0-only
// The attachment browser's open state: a tiny standalone store the overlay
// reads, with module-scope verbs so non-React code (the palette command, a
// future ContactDetail sender entry) can open and close it without a hook.
// The sender prefilter rides here so an opener can narrow the browse to one
// sender; the mime filter is the surface's own session state, never persisted.

import { create } from 'zustand';

interface AttachmentBrowserState {
  /** Whether the overlay stands. */
  isOpen: boolean;
  /** The sender prefilter the browse opens under, or null for every sender. */
  sender: string | null;
  /** Opens the overlay, optionally narrowed to one sender. */
  open: (sender?: string | null) => void;
  /** Closes the overlay and drops the prefilter. */
  close: () => void;
  /** Clears the sender prefilter while the overlay stands. */
  clearSender: () => void;
}

export const useAttachmentBrowserStore = create<AttachmentBrowserState>((set) => ({
  isOpen: false,
  sender: null,
  open: (sender = null) => set({ isOpen: true, sender: sender ?? null }),
  close: () => set({ isOpen: false, sender: null }),
  clearSender: () => set({ sender: null }),
}));

/** Opens the attachment browser from outside React. */
export function openAttachmentBrowser(sender?: string | null): void {
  useAttachmentBrowserStore.getState().open(sender);
}

/** Closes the attachment browser from outside React. */
export function closeAttachmentBrowser(): void {
  useAttachmentBrowserStore.getState().close();
}
