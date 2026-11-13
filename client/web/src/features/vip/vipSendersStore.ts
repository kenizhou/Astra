// SPDX-License-Identifier: GPL-3.0-only
// The device-local VIP sender set: the senders the user marks important,
// persisted to localStorage the way the pin set is. The keys are bare
// sender addresses (lowercased, trimmed), so the mark is the sender's and
// stands across accounts, unlike pin's per-account rowKeyId. A malformed
// blob reads as empty, a storage failure is logged never thrown, and the
// writes are optimistic: the set flips before the write lands. Pure view
// state: no engine write, no intent, no wire type.

import { create } from 'zustand';

const STORAGE_KEY = 'astra.vipSenders';

/** The bare addr-spec a sender address names: trimmed and lowercased, and
 *  lifted out of a display form ("Ada <ada@x.com>") when one arrives, so
 *  every spelling of one sender agrees on one key. A blank address reads
 *  as empty. */
export function normaliseAddress(address: string): string {
  const lowered = address.trim().toLowerCase();
  const angled = lowered.match(/<([^<>]*)>/);
  return (angled !== null ? angled[1]! : lowered).trim();
}

interface VipSendersState {
  /** The VIP senders' bare addresses, normalised. */
  vip: ReadonlySet<string>;
  /** Flips one sender's VIP mark, optimistically. */
  toggle: (address: string) => void;
  /** Marks one sender VIP; an already marked sender stays untouched. */
  add: (address: string) => void;
  /** Clears one sender's VIP mark. */
  remove: (address: string) => void;
  /** Whether the sender stands VIP. */
  has: (address: string) => boolean;
}

/** Parses the persisted blob defensively: a missing key, non-array JSON,
 *  or non-string entries all read as "no VIP senders"; malformed data
 *  never crashes the list. Well-formed entries normalise on the way in. */
function readVip(): Set<string> {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (raw === null) {
    return new Set();
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return new Set();
    }
    const vip = new Set<string>();
    for (const entry of parsed) {
      if (typeof entry !== 'string') {
        continue;
      }
      const address = normaliseAddress(entry);
      if (address !== '') {
        vip.add(address);
      }
    }
    return vip;
  } catch {
    return new Set();
  }
}

/** Persists the set; a storage failure is logged, never thrown. */
function persist(vip: ReadonlySet<string>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...vip]));
  } catch (err) {
    console.error('vip: the VIP senders could not be persisted', err);
  }
}

export const useVipSendersStore = create<VipSendersState>((set, get) => ({
  vip: readVip(),

  toggle: (address) => {
    const key = normaliseAddress(address);
    if (key === '') {
      return;
    }
    set((state) => {
      const vip = new Set(state.vip);
      if (vip.has(key)) {
        vip.delete(key);
      } else {
        vip.add(key);
      }
      persist(vip);
      return { vip };
    });
  },

  add: (address) => {
    const key = normaliseAddress(address);
    if (key === '') {
      return;
    }
    set((state) => {
      if (state.vip.has(key)) {
        return state;
      }
      const vip = new Set(state.vip);
      vip.add(key);
      persist(vip);
      return { vip };
    });
  },

  remove: (address) => {
    const key = normaliseAddress(address);
    if (key === '') {
      return;
    }
    set((state) => {
      if (!state.vip.has(key)) {
        return state;
      }
      const vip = new Set(state.vip);
      vip.delete(key);
      persist(vip);
      return { vip };
    });
  },

  has: (address) => get().vip.has(normaliseAddress(address)),
}));