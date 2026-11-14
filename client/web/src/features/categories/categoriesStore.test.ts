// SPDX-License-Identifier: GPL-3.0-only
// The category definitions store: the loader pulls the defs and bumps the
// revision, and the manager gate opens and closes. The service is mocked
// whole so the store reads a preset answer.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const service = vi.hoisted(() => ({ listCategories: vi.fn() }));

vi.mock('@/services/categories', () => ({
  listCategories: service.listCategories,
}));

import { useCategoriesStore } from './categoriesStore';

beforeEach(() => {
  service.listCategories.mockReset();
  useCategoriesStore.setState({ defs: [], revision: 0, loaded: false, dialogOpen: false });
});

describe('the category definitions store', () => {
  it('loads the defs, marks loaded, and bumps the revision', async () => {
    const defs = [{ id: 'work', name: 'Work', color: '#FF0000' }];
    service.listCategories.mockResolvedValue(defs);

    await useCategoriesStore.getState().loadCategories();

    const state = useCategoriesStore.getState();
    expect(state.defs).toEqual(defs);
    expect(state.loaded).toBe(true);
    expect(state.revision).toBe(1);
  });

  it('bumps the revision on every load', async () => {
    service.listCategories.mockResolvedValue([]);
    await useCategoriesStore.getState().loadCategories();
    await useCategoriesStore.getState().loadCategories();
    expect(useCategoriesStore.getState().revision).toBe(2);
  });

  it('opens and closes the manager gate', () => {
    expect(useCategoriesStore.getState().dialogOpen).toBe(false);
    useCategoriesStore.getState().open();
    expect(useCategoriesStore.getState().dialogOpen).toBe(true);
    useCategoriesStore.getState().close();
    expect(useCategoriesStore.getState().dialogOpen).toBe(false);
  });
});