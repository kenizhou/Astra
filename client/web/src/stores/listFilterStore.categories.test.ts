// SPDX-License-Identifier: GPL-3.0-only
// The category filter's state: the picker records a category, null clears
// it, and the folder switch's reset clears it beside the tab.
import { beforeEach, describe, expect, it } from 'vitest';

import { useListFilterStore } from './listFilterStore';

beforeEach(() => {
  useListFilterStore.setState({ filter: 'all', categoryFilter: null });
});

describe('the category filter state', () => {
  it('records the category the picker chose', () => {
    useListFilterStore.getState().setCategoryFilter('work');
    expect(useListFilterStore.getState().categoryFilter).toBe('work');
  });

  it('clears the narrowing on null', () => {
    useListFilterStore.getState().setCategoryFilter('work');
    useListFilterStore.getState().setCategoryFilter(null);
    expect(useListFilterStore.getState().categoryFilter).toBeNull();
  });

  it('resets the category beside the tab on a folder switch', () => {
    useListFilterStore.getState().setFilter('unread');
    useListFilterStore.getState().setCategoryFilter('work');
    useListFilterStore.getState().reset();
    expect(useListFilterStore.getState().filter).toBe('all');
    expect(useListFilterStore.getState().categoryFilter).toBeNull();
  });
});