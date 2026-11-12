// SPDX-License-Identifier: GPL-3.0-only
// The device-local pin set: the row identities the user floats to the top
// of the list, persisted to localStorage the way the notification history
// is. The ids are rowKeyId strings (kind, account, then the value half), so
// pins scope per account for free. A malformed blob reads as empty, a
// storage failure is logged never thrown, and the toggle is optimistic: the
// set flips before the write lands. Pure view state: no engine write, no
// intent, no wire type.

import { create } from 'zustand';

const STORAGE_KEY = 'astra.pinnedRows';

interface PinnedRowsState {
  /** The pinned row identities, as rowKeyId strings. */
  pinned: ReadonlySet<string>;
  /** Flips one row's pin, optimistically. */
  toggle: (key: string) => void;
  /** Whether the row identity stands pinned. */
  isPinned: (key: string) => boolean;
}

/** Parses the persisted blob defensively: a missing key, non-array JSON, or
 *  non-string entries all read as "nothing pinned"; malformed data never
 *  crashes the list. */
function readPinned(): Set<string> {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (raw === null) {
    return new Set();
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return new Set();
    }
    return new Set(parsed.filter((v): v is string => typeof v === 'string' && v !== ''));
  } catch {
    return new Set();
  }
}

/** Persists the set; a storage failure is logged, never thrown. */
function persist(pinned: ReadonlySet<string>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...pinned]));
  } catch (err) {
    console.error('list-pin: the pins could not be persisted', err);
  }
}

export const usePinnedRowsStore = create<PinnedRowsState>((set, get) => ({
  pinned: readPinned(),

  toggle: (key) =>
    set((state) => {
      const pinned = new Set(state.pinned);
      if (pinned.has(key)) {
        pinned.delete(key);
      } else {
        pinned.add(key);
      }
      persist(pinned);
      return { pinned };
    }),

  isPinned: (key) => get().pinned.has(key),
}));
