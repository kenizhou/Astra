// SPDX-License-Identifier: GPL-3.0-only
// The Categorize submenu: the Outlook toggle (remove from all when every
// target carries the category, else add to the missing), the checked swatch
// state, the menu staying open across picks, and the manager entry. The
// service is mocked whole; the defs ride the real store.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const service = vi.hoisted(() => ({
  assignCategory: vi.fn(),
  unassignCategory: vi.fn(),
}));

vi.mock('@/services/categories', () => ({
  assignCategory: service.assignCategory,
  unassignCategory: service.unassignCategory,
}));

import { categorizeItem, toggleCategory } from './categoryMenu';
import { useCategoriesStore } from './categoriesStore';
import type { MessageRowView, RowKey } from '@/services/surfaces';

function row(key: RowKey, categories?: string[]): MessageRowView {
  return {
    key,
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'One',
    sent_at: '2026-10-09T08:30:00Z',
    unread: false,
    flagged: false,
    has_attachments: false,
    ...(categories !== undefined ? { categories } : {}),
  };
}

const m1 = row({ message: { account: 'work', key: 'k1' } });
const m2 = row({ message: { account: 'work', key: 'k2' } });

beforeEach(() => {
  service.assignCategory.mockReset();
  service.unassignCategory.mockReset();
  useCategoriesStore.setState({
    defs: [
      { id: 'work', name: 'Work', color: '#FF0000' },
      { id: 'personal', name: 'Personal', color: '#00FF00' },
    ],
    revision: 0,
    loaded: true,
    dialogOpen: false,
  });
});

describe('the Outlook toggle', () => {
  it('removes from all when every target carries the category', () => {
    const a = row(m1.key, ['work']);
    const b = row(m2.key, ['work']);
    toggleCategory([a, b], 'work');
    expect(service.unassignCategory).toHaveBeenCalledTimes(2);
    expect(service.unassignCategory).toHaveBeenCalledWith(m1.key, 'work');
    expect(service.unassignCategory).toHaveBeenCalledWith(m2.key, 'work');
    expect(service.assignCategory).not.toHaveBeenCalled();
  });

  it('adds to the missing when not every target carries it', () => {
    const a = row(m1.key, ['work']);
    const b = row(m2.key, []);
    toggleCategory([a, b], 'work');
    expect(service.assignCategory).toHaveBeenCalledTimes(1);
    expect(service.assignCategory).toHaveBeenCalledWith(m2.key, 'work');
    expect(service.unassignCategory).not.toHaveBeenCalled();
  });

  it('adds to every target when none carries it', () => {
    toggleCategory([m1, m2], 'work');
    expect(service.assignCategory).toHaveBeenCalledTimes(2);
    expect(service.unassignCategory).not.toHaveBeenCalled();
  });
});

describe('the Categorize submenu', () => {
  it('renders a checked swatch per def, checked only when every target carries it', () => {
    const item = categorizeItem([row(m1.key, ['work']), row(m2.key, ['work'])]);
    expect(item.label).toBe('Categorize');
    const work = item.children!.find((child) => child.label === 'Work')!;
    const personal = item.children!.find((child) => child.label === 'Personal')!;
    expect(work.checked).toBe(true);
    expect(work.swatch).toBe('#FF0000');
    expect(personal.checked).toBe(false);
  });

  it('keeps the menu open across picks', () => {
    const item = categorizeItem([m1]);
    const work = item.children!.find((child) => child.label === 'Work')!;
    expect(work.keepOpenOnSelect).toBe(true);
  });

  it('toggles the category when a def is chosen', () => {
    const item = categorizeItem([m1]);
    item.children!.find((child) => child.label === 'Work')!.onSelect!();
    expect(service.assignCategory).toHaveBeenCalledWith(m1.key, 'work');
  });

  it('offers the manager entry that opens the dialog', () => {
    const item = categorizeItem([m1]);
    const manage = item.children!.find((child) => child.label === 'Manage categories…')!;
    manage.onSelect!();
    expect(useCategoriesStore.getState().dialogOpen).toBe(true);
  });
});