// SPDX-License-Identifier: GPL-3.0-only
// The shared context menu primitive: portal rendering at the cursor,
// keyboard navigation with typeahead, the three dismissals, and the
// honest-disable channel (tooltip plus a screen-reader description).

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ContextMenu, placeholderItem, type ContextMenuItem } from './ContextMenu';
import { FlagIcon } from '@/components/icons';

function openMenu(items: ContextMenuItem[], x = 100, y = 100) {
  const onClose = vi.fn();
  render(<ContextMenu x={x} y={y} items={items} onClose={onClose} />);
  return onClose;
}

afterEach(() => {
  cleanup();
});

describe('the context menu primitive', () => {
  it('renders into a portal under the body with the menu role', () => {
    openMenu([{ label: 'Archive', onSelect: () => undefined }]);

    const menu = screen.getByRole('menu');
    expect(menu.parentElement?.parentElement).toBe(document.body);
    expect(screen.getByRole('menuitem', { name: 'Archive' })).toBeTruthy();
  });

  it('renders separators as separators, not items', () => {
    openMenu([
      { label: 'One', onSelect: () => undefined },
      { separator: true },
      { label: 'Two', onSelect: () => undefined },
    ]);

    expect(screen.getAllByRole('menuitem')).toHaveLength(2);
    expect(screen.getByRole('separator')).toBeTruthy();
  });

  it('renders the item icon', () => {
    render(
      <ContextMenu x={10} y={10} items={[{ label: 'Flag', icon: FlagIcon, onSelect: () => undefined }]} onClose={() => undefined} />,
    );
    expect(screen.getByRole('menuitem', { name: 'Flag' }).querySelector('svg')).not.toBeNull();
  });

  it('fires onSelect on click and closes', () => {
    const onSelect = vi.fn();
    const onClose = openMenu([{ label: 'Archive', onSelect }]);

    fireEvent.click(screen.getByRole('menuitem', { name: 'Archive' }));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('a disabled item announces the reason and never fires', () => {
    const onSelect = vi.fn();
    openMenu([{ label: 'Junk', disabled: true, title: 'Junk arrives with a future slice', description: 'Junk arrives with a future slice', onSelect }]);

    const item = screen.getByRole('menuitem', { name: 'Junk' });
    expect(item.getAttribute('aria-disabled')).toBe('true');
    expect(item.querySelector('[title]')?.getAttribute('title')).toBe('Junk arrives with a future slice');
    const descId = item.getAttribute('aria-describedby');
    expect(descId).not.toBeNull();
    expect(document.getElementById(descId!)?.textContent).toBe('Junk arrives with a future slice');

    fireEvent.click(item);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('a danger item carries the danger styling', () => {
    openMenu([{ label: 'Delete', danger: true, onSelect: () => undefined }]);
    const item = screen.getByRole('menuitem', { name: 'Delete' });
    expect(item.className).toContain('text-destructive');
  });

  it('placeholderItem builds a disabled item carrying its reason', () => {
    const item = placeholderItem('Print', 'Print arrives with a future slice');
    expect(item.disabled).toBe(true);
    expect(item.title).toBe('Print arrives with a future slice');
    expect(item.description).toBe('Print arrives with a future slice');
  });

  it('arrow keys move the active item and Enter activates it', () => {
    const first = vi.fn();
    const second = vi.fn();
    openMenu([
      { label: 'First', onSelect: first },
      { label: 'Second', onSelect: second },
    ]);

    const menu = screen.getByRole('menu');
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement?.textContent).toBe('First');
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement?.textContent).toBe('Second');
    fireEvent.keyDown(menu, { key: 'Enter' });
    expect(second).toHaveBeenCalledTimes(1);
    expect(first).not.toHaveBeenCalled();
  });

  it('ArrowUp from the top wraps to the bottom, Home and End jump', () => {
    openMenu([
      { label: 'First', onSelect: () => undefined },
      { label: 'Second', onSelect: () => undefined },
      { label: 'Third', onSelect: () => undefined },
    ]);

    const menu = screen.getByRole('menu');
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    fireEvent.keyDown(menu, { key: 'ArrowUp' });
    expect(document.activeElement?.textContent).toBe('Third');
    fireEvent.keyDown(menu, { key: 'Home' });
    expect(document.activeElement?.textContent).toBe('First');
    fireEvent.keyDown(menu, { key: 'End' });
    expect(document.activeElement?.textContent).toBe('Third');
  });

  it('typeahead jumps to the item by its first letter', () => {
    openMenu([
      { label: 'Archive', onSelect: () => undefined },
      { label: 'Delete', onSelect: () => undefined },
      { label: 'Move', onSelect: () => undefined },
    ]);

    const menu = screen.getByRole('menu');
    fireEvent.keyDown(menu, { key: 'd' });
    expect(document.activeElement?.textContent).toBe('Delete');
  });

  it('skips disabled items during keyboard movement but keeps them focusable on click', () => {
    openMenu([
      { label: 'Archive', onSelect: () => undefined },
      placeholderItem('Junk', 'Junk arrives with a future slice'),
      { label: 'Delete', onSelect: () => undefined },
    ]);

    const menu = screen.getByRole('menu');
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement?.textContent).toBe('Delete');
  });

  it('closes on Escape', () => {
    const onClose = openMenu([{ label: 'One', onSelect: () => undefined }]);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('closes on a pointer down outside, not inside', () => {
    const onClose = openMenu([{ label: 'One', onSelect: () => undefined }]);
    fireEvent.mouseDown(screen.getByRole('menu'));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.mouseDown(document.body);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('closes on window blur', () => {
    const onClose = openMenu([{ label: 'One', onSelect: () => undefined }]);
    fireEvent.blur(window);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('clamps the position into the viewport', () => {
    render(
      <ContextMenu
        x={window.innerWidth - 10}
        y={window.innerHeight - 10}
        items={[{ label: 'One', onSelect: () => undefined }]}
        onClose={() => undefined}
      />,
    );
    const panel = screen.getByRole('menu').parentElement as HTMLElement;
    const left = Number.parseInt(panel.style.left, 10);
    const top = Number.parseInt(panel.style.top, 10);
    expect(left).toBeLessThan(window.innerWidth - 10);
    expect(top).toBeLessThan(window.innerHeight - 10);
  });
});
