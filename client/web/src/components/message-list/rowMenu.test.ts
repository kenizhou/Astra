// SPDX-License-Identifier: GPL-3.0-only
// The row menu's builder: the ported order, the flip labels, the disabled
// reasons on both channels, the batch loops dispatching one intent per
// target, and the two reading-batch verbs: the own-window opening (single
// message only) and the spam report (offered only where the account can
// report).

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));
const window = vi.hoisted(() => ({ openMessageWindow: vi.fn() }));
const draftOpen = vi.hoisted(() => ({
  openDraftRow: vi.fn(),
  openDraftRowWindow: vi.fn(),
}));
const sweep = vi.hoisted(() => ({ open: vi.fn() }));
const snooze = vi.hoisted(() => ({ open: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

vi.mock('@/features/reader/readerWindows', () => ({
  openMessageWindow: window.openMessageWindow,
}));

vi.mock('@/features/composer/draftOpen', () => ({
  openDraftRow: draftOpen.openDraftRow,
  openDraftRowWindow: draftOpen.openDraftRowWindow,
}));

vi.mock('@/features/sweep/sweepStore', () => ({
  useSweepStore: { getState: () => ({ open: sweep.open }) },
}));

vi.mock('@/features/snooze/snoozeStore', () => ({
  useSnoozeStore: { getState: () => ({ open: snooze.open }) },
}));

import { buildDraftMenu, buildRowMenu, type RowAccountContext } from './rowMenu';
import type { FolderRow, MessageRowView, RowKey } from '@/services/surfaces';

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

/** The account context the menu holds: one destination folder, a junk
 * folder the report files into, and a provider that can report its
 * mail. */
const accountCtx = {
  account: 'work',
  currentKey: 'INBOX',
  canReportMail: true,
  folders: [
    { key: 'Archive', name: 'Archive', unread: 0, depth: 0, has_children: false, expanded: false },
    { key: 'Junk', name: 'Junk', role: 'junk', unread: 0, depth: 0, has_children: false, expanded: false },
  ] as FolderRow[],
} satisfies RowAccountContext;

/** The labels of the menu's items, separators as '|'. */
function labels(items: ReturnType<typeof buildRowMenu>): string[] {
  return items.map((item) => (item.separator ? '|' : (item.label ?? '?')));
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  window.openMessageWindow.mockReset();
  window.openMessageWindow.mockResolvedValue(undefined);
  draftOpen.openDraftRow.mockReset();
  draftOpen.openDraftRowWindow.mockReset();
  sweep.open.mockReset();
  snooze.open.mockReset();
});

// The confirm spy restores after each test, so a refused click in one
// leaves the next its own answer.
afterEach(() => {
  vi.restoreAllMocks();
});

describe('buildRowMenu', () => {
  it('holds the ported order', () => {
    expect(labels(buildRowMenu([m1], 'flat', accountCtx))).toEqual([
      'Open in New Window',
      'Pin',
      'Add VIP',
      'Block sender…',
      '|',
      'Reply',
      'Reply All',
      'Forward',
      'Mark as Unread',
      'Flag',
      '|',
      'Move to folder',
      'Mark as spam',
      '|',
      'Delete',
      'Delete Permanently',
      'Archive',
      'Clean up sender…',
      'Snooze…',
      'Remind me…',
      '|',
      'Copy Subject',
      'Copy Sender',
    ]);
  });

  it('opens the sweep dialog prefilled with the row sender', () => {
    const items = buildRowMenu([m1], 'flat', accountCtx);
    const entry = items.find((item) => item.label === 'Clean up sender…')!;

    expect(entry.disabled).toBeUndefined();
    entry.onSelect!();

    expect(sweep.open).toHaveBeenCalledTimes(1);
    expect(sweep.open).toHaveBeenCalledWith('ada@example.test');
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('disables the sweep entry on a batch, with the reason', () => {
    const items = buildRowMenu([m1, m2], 'flat', accountCtx);
    const entry = items.find((item) => item.label === 'Clean up sender…')!;

    expect(entry.disabled).toBe(true);
    expect(entry.description).toContain('Pick one row');
    entry.onSelect?.();
    expect(sweep.open).not.toHaveBeenCalled();
  });

  it('opens the snooze picker over the single message', () => {
    const items = buildRowMenu([m1], 'flat', accountCtx);
    const entry = items.find((item) => item.label === 'Snooze…')!;

    expect(entry.disabled).toBeUndefined();
    entry.onSelect!();

    expect(snooze.open).toHaveBeenCalledTimes(1);
    expect(snooze.open).toHaveBeenCalledWith({ account: 'work', key: 'k1' });
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('disables the snooze entry on a batch and on a conversation, with the reason', () => {
    const batch = buildRowMenu([m1, m2], 'flat', accountCtx).find(
      (item) => item.label === 'Snooze…',
    )!;
    expect(batch.disabled).toBe(true);
    expect(batch.description).toContain('Pick one message');
    batch.onSelect?.();
    expect(snooze.open).not.toHaveBeenCalled();

    const thread = buildRowMenu([th], 'flat', accountCtx).find(
      (item) => item.label === 'Snooze…',
    )!;
    expect(thread.disabled).toBe(true);
    expect(thread.description).toContain('conversation');
    thread.onSelect?.();
    expect(snooze.open).not.toHaveBeenCalled();
  });

  it('opens the message in its own window from the single-message item', () => {
    const items = buildRowMenu([m1], 'flat', accountCtx);
    const open = items.find((item) => item.label === 'Open in New Window')!;

    expect(open.disabled).toBeUndefined();
    open.onSelect!();

    expect(window.openMessageWindow).toHaveBeenCalledTimes(1);
    expect(window.openMessageWindow).toHaveBeenCalledWith(
      { account: 'work', key: 'k1' },
      'Quarterly report',
      'INBOX',
    );
  });

  it('disables the own-window verb on a thread row and on a batch, with the reason', () => {
    const threadItems = buildRowMenu([th], 'flat', accountCtx);
    const threadOpen = threadItems.find((item) => item.label === 'Open in New Window')!;
    expect(threadOpen.disabled).toBe(true);
    expect(threadOpen.description).toContain('conversation');

    const batchItems = buildRowMenu([m1, m2], 'flat', accountCtx);
    const batchOpen = batchItems.find((item) => item.label === 'Open in New Window')!;
    expect(batchOpen.disabled).toBe(true);
    expect(batchOpen.description).toContain('Pick one message');
    batchOpen.onSelect?.();
    expect(window.openMessageWindow).not.toHaveBeenCalled();
  });

  it('reports the row as spam through the mark_junk intent, one per target', () => {
    const items = buildRowMenu([m1, th], 'threaded', accountCtx);

    items.find((item) => item.label === 'Mark as spam')!.onSelect!();

    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(ipc.dispatch).toHaveBeenCalledWith({ mark_junk: { row: m1.key } });
    expect(ipc.dispatch).toHaveBeenCalledWith({ mark_junk: { row: th.key } });
  });

  it('offers the spam report only where the account can report', () => {
    const incapable = { ...accountCtx, canReportMail: false };
    expect(
      buildRowMenu([m1], 'flat', incapable).find((item) => item.label === 'Mark as spam'),
    ).toBeUndefined();
    expect(buildRowMenu([m1], 'flat').find((item) => item.label === 'Mark as spam')).toBeUndefined();
    expect(
      buildRowMenu([m1], 'flat', accountCtx).find((item) => item.label === 'Mark as spam'),
    ).toBeTruthy();
  });

  it('offers the move-to submenu, the sitting folder out', () => {
    const folders: FolderRow[] = [
      { key: 'INBOX', name: 'Inbox', role: 'inbox', unread: 0, depth: 0, has_children: false, expanded: false },
      { key: 'Archive', name: 'Archive', unread: 0, depth: 0, has_children: false, expanded: false },
      { key: 'Clients', name: 'Clients', unread: 0, depth: 0, has_children: false, expanded: false },
    ];
    const items = buildRowMenu([m1], 'flat', { ...accountCtx, folders });

    const move = items.find((item) => item.label === 'Move to folder')!;
    expect(move.disabled).toBeUndefined();
    expect(move.children?.map((child) => child.label)).toEqual(['Archive', 'Clients']);

    move.children![1]!.onSelect!();
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      move_messages: {
        rows: [{ account: 'work', key: 'k1' }],
        folder: { account: 'work', key: 'Clients' },
      },
    });
  });

  it('moves the whole selection from the submenu', () => {
    const folders: FolderRow[] = [
      { key: 'Archive', name: 'Archive', unread: 0, depth: 0, has_children: false, expanded: false },
    ];
    const items = buildRowMenu([m1, m2], 'flat', { ...accountCtx, folders });

    items.find((item) => item.label === 'Move to folder')!.children![0]!.onSelect!();
    expect(ipc.dispatch).toHaveBeenCalledWith({
      move_messages: {
        rows: [
          { account: 'work', key: 'k1' },
          { account: 'work', key: 'k2' },
        ],
        folder: { account: 'work', key: 'Archive' },
      },
    });
  });

  it('hides the move-to verb while no other folder stands in the account', () => {
    const folders: FolderRow[] = [
      { key: 'INBOX', name: 'Inbox', role: 'inbox', unread: 0, depth: 0, has_children: false, expanded: false },
    ];
    const items = buildRowMenu([m1], 'flat', { ...accountCtx, folders });
    expect(items.find((item) => item.label === 'Move to folder')).toBeUndefined();
  });

  it('hides the move-to verb for a selection that spans accounts', () => {
    const other = row({ message: { account: 'home', key: 'k9' } }, { account_id: 'home' });
    const items = buildRowMenu([m1, other], 'flat', accountCtx);
    expect(items.find((item) => item.label === 'Move to folder')).toBeUndefined();
  });

  it('disables the move-to verb with the conversation reason on a thread target', () => {
    const items = buildRowMenu([th], 'threaded', accountCtx);
    expect(items.find((item) => item.label === 'Move to folder')).toEqual({
      label: 'Move to folder',
      disabled: true,
      title: 'A whole conversation cannot be moved for this yet',
      description: 'A whole conversation cannot be moved for this yet',
    });
  });

  it('holds no move-to item while the picker has no context', () => {
    const items = buildRowMenu([m1], 'flat');
    expect(items.find((item) => item.label === 'Move to folder')).toBeUndefined();
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
    const items = buildRowMenu(batch, 'threaded', accountCtx);
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

  it('offers the permanent delete below the ordinary Delete, danger-styled', () => {
    const items = buildRowMenu([m1], 'flat', accountCtx);
    const positions = labels(items);
    expect(positions.indexOf('Delete Permanently')).toBe(positions.indexOf('Delete') + 1);
    expect(items.find((item) => item.label === 'Delete Permanently')!.danger).toBe(true);
    expect(items.find((item) => item.label === 'Delete Permanently')!.disabled).toBeUndefined();
  });

  it('asks the cannot-be-undone confirm, then dispatches the delete intent', () => {
    vi.spyOn(globalThis, 'confirm').mockReturnValue(true);
    const items = buildRowMenu([m1], 'flat', accountCtx);

    items.find((item) => item.label === 'Delete Permanently')!.onSelect!();

    expect(globalThis.confirm).toHaveBeenCalledWith('Permanently delete this message? This cannot be undone.');
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({ delete_message: { row: m1.key } });
  });

  it('words the confirm for a conversation row', () => {
    vi.spyOn(globalThis, 'confirm').mockReturnValue(true);
    const items = buildRowMenu([th], 'threaded', accountCtx);

    items.find((item) => item.label === 'Delete Permanently')!.onSelect!();

    expect(globalThis.confirm).toHaveBeenCalledWith('Permanently delete this conversation? This cannot be undone.');
    expect(ipc.dispatch).toHaveBeenCalledWith({ delete_message: { row: th.key } });
  });

  it('keeps the mail when the permanent delete confirm is refused', () => {
    vi.spyOn(globalThis, 'confirm').mockReturnValue(false);
    const items = buildRowMenu([m1], 'flat', accountCtx);

    items.find((item) => item.label === 'Delete Permanently')!.onSelect!();

    expect(globalThis.confirm).toHaveBeenCalled();
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('never offers the permanent delete as a batch loop', () => {
    const items = buildRowMenu([m1, m2], 'flat', accountCtx);
    const permanent = items.find((item) => item.label === 'Delete Permanently')!;
    expect(permanent.disabled).toBe(true);
    expect(permanent.title).toBe('Pick one row for this');
    permanent.onSelect?.();
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('counts flat-mode batches as messages', () => {
    const items = buildRowMenu([m1, m2], 'flat', accountCtx);
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
  const d1 = row({ message: { account: 'work', key: 'd-1' } }, { subject: 'The half' });

  it('holds Open, Open in New Window, and a dangerous Delete Draft, no placeholders', () => {
    const items = buildDraftMenu([d1]);
    expect(labels(items)).toEqual(['Open', 'Open in New Window', '|', 'Delete Draft']);
    for (const item of items.filter((entry) => !entry.separator)) {
      expect(item.disabled).toBeUndefined();
      expect(item.title).toBeUndefined();
    }
    expect(items.find((item) => item.label === 'Delete Draft')!.danger).toBe(true);
  });

  it('opens the draft row into the composer by key', () => {
    buildDraftMenu([d1]).find((item) => item.label === 'Open')!.onSelect!();
    expect(draftOpen.openDraftRow).toHaveBeenCalledTimes(1);
    expect(draftOpen.openDraftRow).toHaveBeenCalledWith({ account: 'work', key: 'd-1' });
  });

  it('opens the draft row in its own window', () => {
    buildDraftMenu([d1])
      .find((item) => item.label === 'Open in New Window')!
      .onSelect!();
    expect(draftOpen.openDraftRowWindow).toHaveBeenCalledTimes(1);
    expect(draftOpen.openDraftRowWindow).toHaveBeenCalledWith(
      { account: 'work', key: 'd-1' },
      'The half',
    );
  });

  it('deletes the draft by key through the stored discard intent', () => {
    buildDraftMenu([d1]).find((item) => item.label === 'Delete Draft')!.onSelect!();
    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      drafts: { discard_stored: { message: { account: 'work', key: 'd-1' } } },
    });
  });

  it('disables every verb on a batch with the reason', () => {
    const items = buildDraftMenu([m1, m2]);
    for (const item of items.filter((entry) => !entry.separator)) {
      expect(item.disabled).toBe(true);
      expect(item.title).toBe('Pick one draft for this');
      item.onSelect?.();
    }
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(draftOpen.openDraftRow).not.toHaveBeenCalled();
    expect(draftOpen.openDraftRowWindow).not.toHaveBeenCalled();
  });
});
