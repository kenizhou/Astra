// SPDX-License-Identifier: GPL-3.0-only
// The folder drag's own arithmetic: what one dragged folder may legally land
// on, computed over the account's rows before anything dispatches, so an
// illegal target (a drop into the folder's own subtree, a cross-account drop,
// the unified row) is refused before a single intent crosses. Pure functions;
// the tree's own handlers stay thin.

import { describe, expect, it } from 'vitest';

import {
  folderDrop,
  insideTrash,
  messageRefsOf,
  type FolderDropTarget,
} from './folderDrag';
import type { FolderRow, RowKey } from '@/services/surfaces';

function row(key: string, over: Partial<FolderRow> = {}): FolderRow {
  return {
    key,
    name: key,
    unread: 0,
    depth: 0,
    has_children: false,
    expanded: true,
    ...over,
  };
}

const TREE: FolderRow[] = [
  row('INBOX', { role: 'inbox' }),
  row('Projects'),
  row('Projects/Acme', { parent: 'Projects', depth: 1 }),
  row('Projects/Acme/Deep', { parent: 'Projects/Acme', depth: 2 }),
  row('Trash', { role: 'trash' }),
];

const folderTarget = (key: string): FolderDropTarget => ({
  kind: 'folder',
  account: 'work',
  key,
});

describe('folderDrop', () => {
  it('reparents a folder onto a same-account folder', () => {
    expect(folderDrop({ account: 'work', key: 'Projects/Acme' }, folderTarget('Trash'), TREE)).toBe(
      'reparent',
    );
  });

  it('refuses a drop onto itself', () => {
    expect(folderDrop({ account: 'work', key: 'Projects' }, folderTarget('Projects'), TREE)).toBe(
      'refuse',
    );
  });

  it('refuses a drop into the folder own subtree, before anything dispatches', () => {
    expect(
      folderDrop({ account: 'work', key: 'Projects' }, folderTarget('Projects/Acme/Deep'), TREE),
    ).toBe('refuse');
    expect(
      folderDrop({ account: 'work', key: 'Projects' }, folderTarget('Projects/Acme'), TREE),
    ).toBe('refuse');
  });

  it('refuses a cross-account drop, folder or account row', () => {
    expect(folderDrop({ account: 'work', key: 'Projects' }, folderTarget('INBOX'), [
      row('INBOX'),
    ])).toBe('reparent');
    expect(
      folderDrop({ account: 'home', key: 'Projects' }, { kind: 'folder', account: 'work', key: 'INBOX' }, TREE),
    ).toBe('refuse');
    expect(
      folderDrop({ account: 'home', key: 'Projects' }, { kind: 'account', account: 'work' }, TREE),
    ).toBe('refuse');
  });

  it('makes a root of a folder dropped on its own account row', () => {
    expect(
      folderDrop({ account: 'work', key: 'Projects/Acme' }, { kind: 'account', account: 'work' }, TREE),
    ).toBe('make-root');
  });

  it('refuses the unified row outright', () => {
    expect(folderDrop({ account: 'work', key: 'Projects' }, 'unified', TREE)).toBe('refuse');
  });
});

describe('insideTrash', () => {
  it('names the Trash itself and everything filed under it', () => {
    expect(insideTrash('Trash', TREE)).toBe(true);
    expect(insideTrash('Projects', TREE)).toBe(false);
  });
});

describe('messageRefsOf', () => {
  it('keeps the message keys and drops the thread keys', () => {
    const message: RowKey = { message: { account: 'work', key: 'm1' } };
    const thread: RowKey = { thread: { account: 'work', thread_id: 'th-1' } };
    expect(messageRefsOf([message, thread])).toEqual([{ account: 'work', key: 'm1' }]);
    expect(messageRefsOf([thread])).toEqual([]);
  });
});
