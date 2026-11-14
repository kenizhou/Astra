// SPDX-License-Identifier: GPL-3.0-only
// The category wire: each command asks by name with its arguments in the
// camelCase the host's command macro reads, the row reference crossing as the
// RowKey twin the assign and unassign verbs parse.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import {
  assignCategory,
  createCategory,
  deleteCategory,
  listCategories,
  unassignCategory,
  updateCategory,
} from './categories';

beforeEach(() => {
  invoke.mockReset();
});

describe('the category commands', () => {
  it('lists by name with no arguments', async () => {
    invoke.mockResolvedValue([]);
    await listCategories();
    expect(invoke).toHaveBeenCalledWith('categories_list');
  });

  it('creates with the name and colour', async () => {
    invoke.mockResolvedValue({ id: 'work', name: 'Work', color: '#FF0000' });
    await createCategory('Work', '#FF0000');
    expect(invoke).toHaveBeenCalledWith('categories_create', { name: 'Work', color: '#FF0000' });
  });

  it('updates with the id, name, and colour', async () => {
    invoke.mockResolvedValue(undefined);
    await updateCategory('work', 'Projects', '#00FF00');
    expect(invoke).toHaveBeenCalledWith('categories_update', {
      id: 'work',
      name: 'Projects',
      color: '#00FF00',
    });
  });

  it('deletes with the id', async () => {
    invoke.mockResolvedValue(2);
    const removed = await deleteCategory('work');
    expect(invoke).toHaveBeenCalledWith('categories_delete', { id: 'work' });
    expect(removed).toBe(2);
  });

  it('assigns with the row reference and the camelCase category id', async () => {
    invoke.mockResolvedValue(true);
    const row = { message: { account: 'work', key: 'm1' } };
    await assignCategory(row, 'work');
    expect(invoke).toHaveBeenCalledWith('categories_assign', { row, categoryId: 'work' });
  });

  it('unassigns a thread row by its reference', async () => {
    invoke.mockResolvedValue(true);
    const row = { thread: { account: 'work', thread_id: 'th-1' } };
    await unassignCategory(row, 'work');
    expect(invoke).toHaveBeenCalledWith('categories_unassign', { row, categoryId: 'work' });
  });
});