// SPDX-License-Identifier: GPL-3.0-only
// The row menu's builder: the ported order, the flip labels, the disabled
// reasons on both channels, and the batch loops dispatching one intent
// per target.

import { beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { buildDraftMenu, buildRowMenu } from './rowMenu';
import type { MessageRowView, RowKey } from '@/services/surfaces';

function row(key: RowKey, over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key,
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'Quarterly report',
    sent_at: '2026-10-09T08:30:00Z',
    unread: false,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

const m1 = row({ message: { account: 'work', key: 'k1' } });
const m2 = row({ message: { account: 'work', key: 'k2' } });
const th = row({ thread: { account: 'work', thread_id: 'th-1' } });

/** The labels of the menu's items, separators as '|'. */
function labels(items: ReturnType<typeof buildRowMenu>): string[] {
  return items.map((item) => (item.separator ? '|' : (item.label ?? '?')));
}

beforeEach(() => {
  ipc.dispatch.mockReset();
});

describe('buildRowMenu', () => {
  it('holds the ported order', () => {
    expect(labels(buildRowMenu([m1], 'flat'))).toEqual([
      'Open in New Window',
      '|',
      'Reply',
      'Reply All',
      'Forward',
      'Mark as Unread',
      'Flag',
      '|',
      'Move to folder',
      'Junk',
      '|',
      'Delete',
      'Archive',
      '|',
      'Copy Subject',
      'Copy Sender',
    ]);
  });

  it('disables the placeholders with their reasons on both channels', () => {
    const items = buildRowMenu([m1], 'flat');
    const openWindow = items.find((item) => item.label === 'Open in New Window')!;
    expect(openWindow.disabled).toBe(true);
    expect(openWindow.title).toContain('future slice');
    expect(openWindow.description).toBe(openWindow.title);
    const move = items.find((item) => item.label === 'Move to folder')!;
    expect(move.disabled).toBe(true);
    const junk = items.find((item) => item.label === 'Junk')!;
    expect(junk.disabled).toBe(true);
  });

  it('flips Mark as Read on an unread target', () => {
    const items = buildRowMenu([row(m1.key, { unread: true })], 'flat');
    expect(items.find((item) => item.label === 'Mark as Read')).toBeTruthy();
  });

  it('flips Unflag when every target is flagged', () => {
    const items = buildRowMenu([row(m1.key, { flagged: true })], 'flat');
    expect(items.find((item) => item.label === 'Unflag')).toBeTruthy();
  });

  it('marks read per target state across a mixed batch', () => {
    const batch = [row(m1.key, { unread: true }), row(m2.key, { unread: false })];
    const items = buildRowMenu(batch, 'flat');
    items.find((item) => item.label === 'Mark as Read')!.onSelect!();
    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      mark_read: { row: m1.key, read: true },
    });
    expect(ipc.dispatch).toHaveBeenCalledWith({
      mark_read: { row: m2.key, read: false },
    });
  });

  it('dispatches the compose verbs for a single message row', () => {
    const ref = { account: 'work', key: 'k1' };
    const items = buildRowMenu([m1], 'flat');
    items.find((item) => item.label === 'Reply')!.onSelect!();
    expect(ipc.dispatch).toHaveBeenCalledWith({
      compose: { reply: { message: ref } },
    });
    items.find((item) => item.label === 'Reply All')!.onSelect!();
    expect(ipc.dispatch).toHaveBeenCalledWith({
      compose: { reply_all: { message: ref } },
    });
    items.find((item) => item.label === 'Forward')!.onSelect!();
    expect(ipc.dispatch).toHaveBeenCalledWith({
      compose: { forward: { message: ref } },
    });
  });

  it('disables the compose verbs on a thread row and on a batch', () => {
    const threadItems = buildRowMenu([th], 'flat');
    for (const label of ['Reply', 'Reply All', 'Forward']) {
      const item = threadItems.find((entry) => entry.label === label)!;
      expect(item.disabled).toBe(true);
      expect(item.description).toContain('conversation');
    }
    const batchItems = buildRowMenu([m1, m2], 'flat');
    for (const label of ['Reply', 'Reply All', 'Forward']) {
      expect(batchItems.find((entry) => entry.label === label)!.disabled).toBe(true);
    }
  });

  it('loops archive and trash over a batch with counted labels', () => {
    const batch = [m1, m2, th];
    const items = buildRowMenu(batch, 'threaded');
    const del = items.find((item) => item.label === 'Delete 3 conversations')!;
    expect(del.danger).toBe(true);
    del.onSelect!();
    const archive = items.find((item) => item.label === 'Archive 3 conversations')!;
    archive.onSelect!();
    expect(ipc.dispatch).toHaveBeenCalledTimes(6);
    expect(ipc.dispatch).toHaveBeenCalledWith({ move_to_trash: { row: m1.key } });
    expect(ipc.dispatch).toHaveBeenCalledWith({ move_to_trash: { row: th.key } });
    expect(ipc.dispatch).toHaveBeenCalledWith({ archive_message: { row: m2.key } });
  });

  it('counts flat-mode batches as messages', () => {
    const items = buildRowMenu([m1, m2], 'flat');
    expect(items.find((item) => item.label === 'Delete 2 messages')).toBeTruthy();
    expect(items.find((item) => item.label === 'Archive 2 messages')).toBeTruthy();
  });

  it('disables the copy pair on a batch and copies on a single row', () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    const batch = buildRowMenu([m1, m2], 'flat');
    expect(batch.find((item) => item.label === 'Copy Subject')!.disabled).toBe(true);
    expect(batch.find((item) => item.label === 'Copy Sender')!.disabled).toBe(true);

    const single = buildRowMenu([m1], 'flat');
    single.find((item) => item.label === 'Copy Subject')!.onSelect!();
    expect(writeText).toHaveBeenCalledWith('Quarterly report');
    single.find((item) => item.label === 'Copy Sender')!.onSelect!();
    expect(writeText).toHaveBeenCalledWith('ada@example.test');
  });
});

describe('buildDraftMenu', () => {
  it('holds Open, Open in New Window, and a dangerous Delete, all placeholders', () => {
    const items = buildDraftMenu();
    expect(labels(items)).toEqual(['Open', 'Open in New Window', '|', 'Delete Draft']);
    for (const item of items.filter((entry) => !entry.separator)) {
      expect(item.disabled).toBe(true);
      expect(item.title).toContain('future slice');
    }
    expect(items.find((item) => item.label === 'Delete Draft')!.danger).toBe(true);
  });
});
