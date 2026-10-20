// SPDX-License-Identifier: GPL-3.0-only
// The folder pane's context menu builder: sync and the inline create as they
// stand, mark-all-read looping the listed rows, rename handing the tree its
// inline rename row, and delete asking its confirm before the trash (or, in
// the Trash, the permanent delete), with every tree-edit verb hidden while
// the account cannot edit its folders.

import { beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { buildFolderMenu } from './folderMenu';
import type { AccountFolderRow, FolderRow, MessageRowView } from '@/services/surfaces';

function folder(over: Partial<FolderRow> = {}): FolderRow {
  return {
    key: 'Projects',
    name: 'Projects',
    unread: 0,
    depth: 0,
    has_children: false,
    expanded: true,
    ...over,
  };
}

function account(over: Partial<AccountFolderRow> = {}): AccountFolderRow {
  return {
    account_id: 'work',
    display_name: 'me@work.example',
    address: 'me@work.example',
    expanded: true,
    can_edit_folders: true,
    folders: [],
    ...over,
  };
}

/** One message row the mark-all loop names. */
function row(key: string, unread: boolean): MessageRowView {
  return {
    key: { message: { account: 'work', key } },
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'Report',
    sent_at: '2026-10-08T16:05:00Z',
    unread,
    flagged: false,
    has_attachments: false,
  };
}

const noRows = { onCreateSubfolder: () => undefined, onRename: () => undefined, listedRows: null };

beforeEach(() => {
  ipc.dispatch.mockReset();
  vi.restoreAllMocks();
});

describe('buildFolderMenu', () => {
  it('holds the ported order', () => {
    const labels = buildFolderMenu(folder(), account(), {
      ...noRows,
      listedRows: [],
    }).map((item) => (item.separator ? '|' : item.label));
    expect(labels).toEqual([
      'Sync this folder',
      'New Subfolder',
      '|',
      'Mark All as Read',
      'Rename Folder',
      'Delete Folder',
    ]);
  });

  it('loops mark-read over the listed unread rows', () => {
    const items = buildFolderMenu(folder(), account(), {
      ...noRows,
      listedRows: [row('m1', true), row('m2', false), row('m3', true)],
    });
    items.find((item) => item.label === 'Mark All as Read')!.onSelect!();

    expect(ipc.dispatch).toHaveBeenCalledTimes(2);
    expect(ipc.dispatch).toHaveBeenCalledWith({
      mark_read: { row: { message: { account: 'work', key: 'm1' } }, read: true },
    });
    expect(ipc.dispatch).toHaveBeenCalledWith({
      mark_read: { row: { message: { account: 'work', key: 'm3' } }, read: true },
    });
  });

  it('hides mark-all-read while the folder is not the one the list shows', () => {
    const items = buildFolderMenu(folder(), account(), noRows);
    expect(items.find((item) => item.label === 'Mark All as Read')).toBeUndefined();
  });

  it('hands the rename click to the tree', () => {
    const onRename = vi.fn();
    const items = buildFolderMenu(folder(), account(), { ...noRows, onRename });
    items.find((item) => item.label === 'Rename Folder')!.onSelect!();
    expect(onRename).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('keeps the special-folder reason on a role folder', () => {
    const items = buildFolderMenu(folder({ role: 'inbox' }), account(), noRows);
    const rename = items.find((item) => item.label === 'Rename Folder')!;
    expect(rename.disabled).toBe(true);
    expect(rename.title).toBe('Special folders cannot be renamed');
    expect(items.find((item) => item.label === 'Delete Folder')!.title).toBe(
      'Special folders cannot be deleted',
    );
  });

  it('asks a confirm, then trashes, from an ordinary folder', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const items = buildFolderMenu(folder(), account(), noRows);

    items.find((item) => item.label === 'Delete Folder')!.onSelect!();

    expect(window.confirm).toHaveBeenCalled();
    expect(ipc.dispatch).toHaveBeenCalledWith(
      expect.objectContaining({
        folders: { trash: { account: 'work', folder: 'Projects' } },
      }),
    );
  });

  it('keeps the folder when the confirm is refused', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    const items = buildFolderMenu(folder(), account(), noRows);

    items.find((item) => item.label === 'Delete Folder')!.onSelect!();

    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('offers the permanent delete behind its own confirm on the Trash itself', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const trash = folder({ key: 'Trash', name: 'Trash', role: 'trash' });
    const items = buildFolderMenu(
      trash,
      account({ folders: [trash] }),
      noRows,
    );

    items.find((item) => item.label === 'Delete Folder')!.onSelect!();

    expect(window.confirm).toHaveBeenCalled();
    expect(ipc.dispatch).toHaveBeenCalledWith(
      expect.objectContaining({
        folders: { delete: { account: 'work', folder: 'Trash' } },
      }),
    );
  });

  it('offers the permanent delete on a folder filed inside the Trash', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const tree = account({
      folders: [
        folder({ key: 'Trash', name: 'Trash', role: 'trash' }),
        folder({ key: 'Trash/Old', name: 'Old', parent: 'Trash', depth: 1 }),
      ],
    });
    const items = buildFolderMenu(
      tree.folders[1]!,
      tree,
      noRows,
    );

    items.find((item) => item.label === 'Delete Folder')!.onSelect!();

    expect(ipc.dispatch).toHaveBeenCalledWith(
      expect.objectContaining({
        folders: { delete: { account: 'work', folder: 'Trash/Old' } },
      }),
    );
  });

  it('hides every tree-edit verb while the account cannot edit its folders', () => {
    const items = buildFolderMenu(folder(), account({ can_edit_folders: false }), noRows);
    expect(items.find((item) => item.label === 'New Subfolder')).toBeUndefined();
    expect(items.find((item) => item.label === 'Rename Folder')).toBeUndefined();
    expect(items.find((item) => item.label === 'Delete Folder')).toBeUndefined();
    // The verbs the account can still serve stand.
    expect(items.find((item) => item.label === 'Sync this folder')).toBeTruthy();
  });
});
