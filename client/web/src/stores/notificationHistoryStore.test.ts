// SPDX-License-Identifier: GPL-3.0-only
// The notification history store: the batch recording (newest first, the
// dedupe by account and key, the hundred-row cap), the dismiss and clear,
// and the device-local persistence (a malformed blob reads as empty).
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  NOTIFICATION_HISTORY_CAP,
  notificationEntryId,
  useNotificationHistoryStore,
  type NewMailBatchWire,
  type NewMailNoticeWire,
} from './notificationHistoryStore';

/** One wire notice, minted around `key`. */
function notice(key: string, over: Partial<NewMailNoticeWire> = {}): NewMailNoticeWire {
  return {
    account: 'work',
    key,
    from: { name: `Sender ${key}`, email: `${key}@example.test` },
    subject: `Subject ${key}`,
    preview: `Preview ${key}`,
    ...over,
  };
}

/** A wire batch of `keys`, no overflow. */
function batch(keys: string[]): NewMailBatchWire {
  return { messages: keys.map((key) => notice(key)), overflow: 0 };
}

/** The recorded ids, newest first. */
function ids(): string[] {
  return useNotificationHistoryStore.getState().entries.map((entry) => entry.id);
}

beforeEach(() => {
  localStorage.clear();
  useNotificationHistoryStore.setState({ entries: [] });
});

afterEach(() => {
  vi.useRealTimers();
  localStorage.clear();
});

describe('the notification history store', () => {
  it('records a batch newest first with the sender, subject, preview, and time', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-11-12T10:00:00Z'));
    const at = Date.now();
    useNotificationHistoryStore.getState().record(batch(['a', 'b']));

    const entries = useNotificationHistoryStore.getState().entries;
    expect(entries.map((entry) => entry.key)).toEqual(['b', 'a']);
    expect(entries[0]).toEqual({
      id: notificationEntryId('work', 'b'),
      account: 'work',
      key: 'b',
      sender: 'Sender b',
      subject: 'Subject b',
      preview: 'Preview b',
      at,
    });
  });

  it('dedupes by account and key, moving a re-notified message to the front', () => {
    const store = useNotificationHistoryStore.getState();
    store.record(batch(['a', 'b', 'c']));
    store.record({ messages: [notice('a', { subject: 'Again' })], overflow: 0 });

    expect(ids()).toEqual([
      notificationEntryId('work', 'a'),
      notificationEntryId('work', 'c'),
      notificationEntryId('work', 'b'),
    ]);
    const entries = useNotificationHistoryStore.getState().entries;
    expect(entries).toHaveLength(3);
    expect(entries[0]?.subject).toBe('Again');
  });

  it('keeps the same key under two accounts as two rows', () => {
    useNotificationHistoryStore.getState().record({
      messages: [notice('a'), notice('a', { account: 'home' })],
      overflow: 0,
    });
    expect(ids()).toEqual([notificationEntryId('home', 'a'), notificationEntryId('work', 'a')]);
  });

  it('caps the history at a hundred, dropping the oldest', () => {
    const keys = Array.from({ length: NOTIFICATION_HISTORY_CAP + 5 }, (_, i) => `k${i}`);
    useNotificationHistoryStore.getState().record(batch(keys));

    const entries = useNotificationHistoryStore.getState().entries;
    expect(entries).toHaveLength(NOTIFICATION_HISTORY_CAP);
    expect(entries[0]?.key).toBe(`k${NOTIFICATION_HISTORY_CAP + 4}`);
    expect(entries.at(-1)?.key).toBe('k5');
  });

  it('records nothing for an empty batch', () => {
    useNotificationHistoryStore.getState().record(batch(['a']));
    useNotificationHistoryStore.getState().record({ messages: [], overflow: 3 });
    expect(ids()).toEqual([notificationEntryId('work', 'a')]);
  });

  it('falls back to the address when the sender carries no name', () => {
    useNotificationHistoryStore.getState().record({
      messages: [notice('a', { from: { name: null, email: 'bare@example.test' } })],
      overflow: 0,
    });
    expect(useNotificationHistoryStore.getState().entries[0]?.sender).toBe('bare@example.test');
  });

  it('omits the sender, subject, and preview the envelope left absent', () => {
    useNotificationHistoryStore.getState().record({
      messages: [notice('a', { from: null, subject: null, preview: null })],
      overflow: 0,
    });
    const entry = useNotificationHistoryStore.getState().entries[0];
    expect(entry?.sender).toBeUndefined();
    expect(entry?.subject).toBeUndefined();
    expect(entry?.preview).toBeUndefined();
  });

  it('dismisses one entry by id', () => {
    const store = useNotificationHistoryStore.getState();
    store.record(batch(['a', 'b', 'c']));
    store.dismiss(notificationEntryId('work', 'b'));
    expect(ids()).toEqual([notificationEntryId('work', 'c'), notificationEntryId('work', 'a')]);
  });

  it('clears the whole history', () => {
    const store = useNotificationHistoryStore.getState();
    store.record(batch(['a', 'b']));
    store.clear();
    expect(useNotificationHistoryStore.getState().entries).toEqual([]);
  });

  it('persists every change to localStorage', () => {
    const store = useNotificationHistoryStore.getState();
    store.record(batch(['a', 'b']));
    const persisted = JSON.parse(localStorage.getItem('astra.notificationHistory') ?? 'null');
    expect(persisted).toEqual(useNotificationHistoryStore.getState().entries);

    store.dismiss(notificationEntryId('work', 'a'));
    const afterDismiss = JSON.parse(localStorage.getItem('astra.notificationHistory') ?? 'null');
    expect(afterDismiss).toEqual(useNotificationHistoryStore.getState().entries);

    store.clear();
    expect(JSON.parse(localStorage.getItem('astra.notificationHistory') ?? 'null')).toEqual([]);
  });

  it('hydrates from localStorage on load', async () => {
    const stored = [
      {
        id: notificationEntryId('work', 'a'),
        account: 'work',
        key: 'a',
        sender: 'Ada',
        subject: 'Hello',
        at: 1_700_000_000_000,
      },
    ];
    localStorage.setItem('astra.notificationHistory', JSON.stringify(stored));
    vi.resetModules();
    const fresh = await import('./notificationHistoryStore');
    expect(fresh.useNotificationHistoryStore.getState().entries).toEqual(stored);
  });

  it('reads a malformed blob as empty', async () => {
    localStorage.setItem('astra.notificationHistory', '{not json');
    vi.resetModules();
    const fresh = await import('./notificationHistoryStore');
    expect(fresh.useNotificationHistoryStore.getState().entries).toEqual([]);
  });

  it('drops the malformed entries and keeps the well-formed ones', async () => {
    const good = {
      id: notificationEntryId('work', 'a'),
      account: 'work',
      key: 'a',
      at: 1_700_000_000_000,
    };
    localStorage.setItem(
      'astra.notificationHistory',
      JSON.stringify([good, { id: '', at: 1 }, { id: 'x', at: 'soon' }, null, 42]),
    );
    vi.resetModules();
    const fresh = await import('./notificationHistoryStore');
    expect(fresh.useNotificationHistoryStore.getState().entries).toEqual([good]);
  });
});