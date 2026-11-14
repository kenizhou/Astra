// SPDX-License-Identifier: GPL-3.0-only
// The context menu's child renderer: the checked gutter and the colour swatch
// a category child carries, and keepOpenOnSelect leaving the menu open across
// a pick (an ordinary child still closes the whole menu).
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ContextMenu, type ContextMenuItem } from './ContextMenu';

afterEach(() => {
  cleanup();
});

/** Opens a parent's flyout by hovering it, the pointer's own discipline. */
function openSubmenu(items: ContextMenuItem[]) {
  const onClose = vi.fn();
  render(<ContextMenu x={100} y={100} items={items} onClose={onClose} />);
  fireEvent.mouseEnter(screen.getByRole('menuitem', { name: 'Categorize' }));
  return onClose;
}

describe('the category child renderer', () => {
  const children: ContextMenuItem[] = [
    { label: 'Work', checked: true, swatch: '#FF0000', keepOpenOnSelect: true, onSelect: () => undefined },
    { label: 'Personal', checked: false, swatch: '#00FF00', keepOpenOnSelect: true, onSelect: () => undefined },
  ];

  it('draws the check gutter for a checked child and an empty one otherwise', () => {
    openSubmenu([{ label: 'Categorize', children }]);
    const work = screen.getByRole('menuitem', { name: 'Work' });
    const personal = screen.getByRole('menuitem', { name: 'Personal' });
    expect(work.querySelector('svg')).not.toBeNull();
    expect(personal.querySelector('svg')).toBeNull();
  });

  it('draws the colour swatch beside the label', () => {
    openSubmenu([{ label: 'Categorize', children }]);
    const work = screen.getByRole('menuitem', { name: 'Work' });
    const swatch = work.querySelector('span[style]') as HTMLElement;
    expect(swatch.getAttribute('style')).toContain('rgb(255, 0, 0)');
  });

  it('keeps the menu open across a keepOpenOnSelect pick', () => {
    const onSelect = vi.fn();
    const onClose = openSubmenu([
      { label: 'Categorize', children: [{ label: 'Work', keepOpenOnSelect: true, onSelect }] },
    ]);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Work' }));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onClose).not.toHaveBeenCalled();
  });

  it('closes the menu on an ordinary child pick', () => {
    const onSelect = vi.fn();
    const onClose = openSubmenu([
      { label: 'Categorize', children: [{ label: 'Manage', onSelect }] },
    ]);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Manage' }));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});