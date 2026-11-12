// SPDX-License-Identifier: GPL-3.0-only
// The device-local notification history: the mail the host actually
// notified (the new-mail event fires after the enabled/focus gate), recorded
// newest first, deduped by account and key, capped at a hundred, and
// persisted to localStorage the way the appearance choices are. A re-notified
// message moves to the front rather than landing a second row. The overflow
// count rides the event but v1 records the explicit messages only.

import { create } from 'zustand';

/** How many entries the history keeps; the oldest drop off the end. */
export const NOTIFICATION_HISTORY_CAP = 100;

const STORAGE_KEY = 'astra.notificationHistory';

/** One notice as the host's new-mail event carries it (snake_case wire). */
export interface NewMailNoticeWire {
  account: string;
  key: string;
  from: { name: string | null; email: string } | null;
  subject: string | null;
  preview: string | null;
}

/** One drained batch as the host's new-mail event carries it. */
export interface NewMailBatchWire {
  messages: NewMailNoticeWire[];
  overflow: number;
}

/** One recorded notification, the row the bell's panel lists. */
export interface NotificationHistoryEntry {
  /** The dedupe key: the account and provider key joined. */
  id: string;
  account: string;
  key: string;
  sender?: string;
  subject?: string;
  preview?: string;
  /** Epoch ms of the recording. */
  at: number;
}

interface NotificationHistoryState {
  /** The recorded notifications, newest first. */
  entries: NotificationHistoryEntry[];
  /** Records a notified batch, deduping by id and capping the list. */
  record: (batch: NewMailBatchWire) => void;
  /** Removes one entry by id. */
  dismiss: (id: string) => void;
  /** Empties the history. */
  clear: () => void;
}

/** The dedupe key for a notice: its account and provider key joined. */
export function notificationEntryId(account: string, key: string): string {
  return `${account}:${key}`;
}

/** The sender label: the display name the envelope carried, else the address. */
function senderLabel(from: NewMailNoticeWire['from']): string | undefined {
  if (from === null) {
    return undefined;
  }
  const name = from.name?.trim();
  return name !== undefined && name !== '' ? name : from.email;
}

/** Maps one wire notice to an entry, omitting the parts the envelope left absent. */
function toEntry(notice: NewMailNoticeWire, at: number): NotificationHistoryEntry {
  const sender = senderLabel(notice.from);
  return {
    id: notificationEntryId(notice.account, notice.key),
    account: notice.account,
    key: notice.key,
    ...(sender !== undefined ? { sender } : {}),
    ...(notice.subject !== null ? { subject: notice.subject } : {}),
    ...(notice.preview !== null ? { preview: notice.preview } : {}),
    at,
  };
}

/** Parses one persisted entry; null when it is not well-formed. */
function parseEntry(value: unknown): NotificationHistoryEntry | null {
  if (typeof value !== 'object' || value === null) {
    return null;
  }
  const { id, account, key, sender, subject, preview, at } = value as Record<string, unknown>;
  if (typeof id !== 'string' || id === '') {
    return null;
  }
  if (typeof account !== 'string' || typeof key !== 'string') {
    return null;
  }
  if (typeof at !== 'number' || !Number.isFinite(at)) {
    return null;
  }
  return {
    id,
    account,
    key,
    at,
    ...(typeof sender === 'string' && sender !== '' ? { sender } : {}),
    ...(typeof subject === 'string' && subject !== '' ? { subject } : {}),
    ...(typeof preview === 'string' && preview !== '' ? { preview } : {}),
  };
}

/** Parses the persisted blob defensively; malformed data reads as empty. */
function readHistory(): NotificationHistoryEntry[] {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (raw === null) {
    return [];
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed.map(parseEntry).filter((entry): entry is NotificationHistoryEntry => entry !== null);
  } catch {
    return [];
  }
}

/** Persists the list; a storage failure is logged, never thrown. */
function persist(entries: NotificationHistoryEntry[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
  } catch (err) {
    console.error('notification-history: the history could not be persisted', err);
  }
}

export const useNotificationHistoryStore = create<NotificationHistoryState>((set) => ({
  entries: readHistory(),

  record: (batch) =>
    set((state) => {
      if (batch.messages.length === 0) {
        return state;
      }
      const at = Date.now();
      // The batch arrives oldest first; folding each notice to the front
      // leaves the newest first, a re-notified id moving ahead of its row.
      let entries = state.entries;
      for (const notice of batch.messages) {
        const entry = toEntry(notice, at);
        entries = [entry, ...entries.filter((existing) => existing.id !== entry.id)];
      }
      entries = entries.slice(0, NOTIFICATION_HISTORY_CAP);
      persist(entries);
      return { entries };
    }),

  dismiss: (id) =>
    set((state) => {
      const entries = state.entries.filter((entry) => entry.id !== id);
      persist(entries);
      return { entries };
    }),

  clear: () => {
    persist([]);
    set({ entries: [] });
  },
}));