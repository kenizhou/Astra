// SPDX-License-Identifier: GPL-3.0-only
// The send-result toast: the sending surface's quiet half. A queued send the
// drain delivers leaves the queue with nobody watching, so the toast watches
// the store the surface pulls land in and says the one honest word when a
// send goes without this session's own hand removing it.

import { useEffect } from 'react';

import { useSendingStore } from '@/stores/sendingStore';

import { SEND_TOAST_MS, useSendResultStore, vanishedKeys } from './sendResult';

/** Watches the queue and shows the send result for its moment. */
export function SendResultToast() {
  const toast = useSendResultStore((s) => s.toast);

  // The transition watcher: every pull the surface triggers replaces the
  // store, and a row held before and gone after is a send that finished.
  useEffect(() => {
    return useSendingStore.subscribe((state, prev) => {
      const settled = useSendResultStore.getState().settled;
      const delivered = vanishedKeys(prev.snapshot.rows, state.snapshot.rows).filter(
        (key) => !settled.includes(key),
      );
      if (delivered.length > 0) {
        useSendResultStore.getState().show('Message sent');
      }
    });
  }, []);

  useEffect(() => {
    if (toast === null) {
      return;
    }
    const timer = setTimeout(() => useSendResultStore.getState().dismiss(), SEND_TOAST_MS);
    return () => clearTimeout(timer);
  }, [toast]);

  if (toast === null) {
    return null;
  }

  return (
    <div
      role="status"
      className="absolute bottom-[calc(var(--status-h)+12px)] left-3 z-[var(--z-toast)] rounded-[var(--radius)] border border-border bg-surface px-3 py-2 text-xs text-foreground shadow-lg"
    >
      {toast}
    </div>
  );
}
