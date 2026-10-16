// SPDX-License-Identifier: GPL-3.0-only
// The folder pane's context menu builder: the ported order (sync, new
// subfolder, then the placeholders), with rename and delete carrying the
// role-folder reason where kylins force-disables them anyway.

import { beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { buildFolderMenu } from './folderMenu';
import type { FolderRow } from '@/services/surfaces';

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

beforeEach(() => {
  ipc.dispatch.mockReset();
});

describe('buildFolderMenu', () => {
  it('holds the ported order', () => {
    const labels = buildFolderMenu(folder(), () => undefined).map((item) =>
      item.separator ? '|' : item.label,
    );
    expect(labels).toEqual([
      'Sync this folder',
      'New Subfolder',
      '|',
      'Mark All as Read',
      'Rename Folder',
      'Delete Folder',
    ]);
  });

  it('dispatches refresh_mail from the sync item', () => {
    const items = buildFolderMenu(folder(), () => undefined);
    items.find((item) => item.label === 'Sync this folder')!.onSelect!();
    expect(ipc.dispatch).toHaveBeenCalledWith('refresh_mail');
  });

  it('hands the new-subfolder click to the tree', () => {
    const onCreate = vi.fn();
    const items = buildFolderMenu(folder(), onCreate);
    items.find((item) => item.label === 'New Subfolder')!.onSelect!();
    expect(onCreate).toHaveBeenCalledTimes(1);
  });

  it('places the future-slice reason on the placeholders of an ordinary folder', () => {
    const items = buildFolderMenu(folder(), () => undefined);
    for (const label of ['Mark All as Read', 'Rename Folder', 'Delete Folder']) {
      const item = items.find((entry) => entry.label === label)!;
      expect(item.disabled).toBe(true);
      expect(item.title).toContain('future slice');
      expect(item.description).toBe(item.title);
    }
    expect(items.find((item) => item.label === 'Delete Folder')!.danger).toBe(true);
  });

  it('names the special-folder reason on a role folder instead', () => {
    const items = buildFolderMenu(folder({ role: 'inbox' }), () => undefined);
    const rename = items.find((item) => item.label === 'Rename Folder')!;
    expect(rename.disabled).toBe(true);
    expect(rename.title).toBe('Special folders cannot be renamed');
    expect(rename.description).toBe('Special folders cannot be renamed');
    const del = items.find((item) => item.label === 'Delete Folder')!;
    expect(del.title).toBe('Special folders cannot be deleted');
  });
});
