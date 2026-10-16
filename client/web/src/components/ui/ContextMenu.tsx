// SPDX-License-Identifier: GPL-3.0-only
// The shared context menu: a floating panel of actions rendered in a portal
// at the cursor, with keyboard navigation (roving tabindex, arrows, Home and
// End, typeahead), the three dismissals (Escape, pointer down outside,
// window blur), and the honest-disable channel — a disabled item names its
// reason in a tooltip and a screen-reader description, and stays in the
// keyboard order so the reason is discoverable. Flyout submenus and
// checkbox items arrive with the first consumer that needs them.

import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

export interface ContextMenuItem {
  label?: string;
  icon?: React.ComponentType<{ size?: number; className?: string }>;
  onSelect?: () => void;
  disabled?: boolean;
  danger?: boolean;
  separator?: boolean;
  /** Native tooltip over the label: the honest-disable channel. */
  title?: string;
  /** The screen-reader leg of that channel, wired via aria-describedby. */
  description?: string;
}

interface ContextMenuProps {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}

/** One disabled menu item carrying its reason on both channels. */
export function placeholderItem(label: string, reason: string): ContextMenuItem {
  return { label, disabled: true, title: reason, description: reason };
}

// Description ids are minted per described item object from a module
// counter (WeakMap-keyed), so two open menus never collide on a DOM id.
let descriptionIdSeq = 0;
const descriptionIds = new WeakMap<ContextMenuItem, string>();

function descriptionId(item: ContextMenuItem): string {
  let id = descriptionIds.get(item);
  if (id === undefined) {
    descriptionIdSeq += 1;
    id = `astra-menu-desc-${descriptionIdSeq}`;
    descriptionIds.set(item, id);
  }
  return id;
}

const ITEM_ESTIMATE_H = 36;
const PANEL_W = 200;

export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [activeIndex, setActiveIndex] = useState(-1);
  const typeahead = useRef({ text: '', at: 0 });

  const actionable = items
    .map((item, index) => ({ item, index }))
    .filter(({ item }) => !item.separator && !item.disabled);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        onClose();
      }
    }
    function onPointer(event: MouseEvent) {
      if (ref.current !== null && !ref.current.contains(event.target as Node)) {
        onClose();
      }
    }
    document.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onPointer);
    window.addEventListener('blur', onClose);
    return () => {
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onPointer);
      window.removeEventListener('blur', onClose);
    };
  }, [onClose]);

  // The active item holds the focus; the roving tabindex follows.
  useEffect(() => {
    if (activeIndex >= 0) {
      ref.current
        ?.querySelector<HTMLElement>(`[data-menu-index="${activeIndex}"]`)
        ?.focus();
    }
  }, [activeIndex]);

  const move = (from: number, delta: number): void => {
    if (actionable.length === 0) {
      return;
    }
    const at = actionable.findIndex(({ index }) => index === from);
    const next = actionable[(at + delta + actionable.length) % actionable.length];
    setActiveIndex(next!.index);
  };

  const onMenuKeyDown = (event: React.KeyboardEvent): void => {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      move(activeIndex, 1);
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      move(activeIndex, -1);
      return;
    }
    if (event.key === 'Home') {
      event.preventDefault();
      if (actionable.length > 0) {
        setActiveIndex(actionable[0]!.index);
      }
      return;
    }
    if (event.key === 'End') {
      event.preventDefault();
      if (actionable.length > 0) {
        setActiveIndex(actionable[actionable.length - 1]!.index);
      }
      return;
    }
    if (event.key === 'Enter' || event.key === ' ') {
      if (activeIndex >= 0) {
        event.preventDefault();
        activate(items[activeIndex]!);
      }
      return;
    }
    // Typeahead: a short buffer of printable characters jumps to the first
    // enabled item whose label starts with it.
    if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      const now = Date.now();
      const buffer = typeahead.current;
      buffer.text = now - buffer.at < 700 ? buffer.text + event.key : event.key;
      buffer.at = now;
      const needle = buffer.text.toLowerCase();
      const hit = actionable.find(({ item }) =>
        (item.label ?? '').toLowerCase().startsWith(needle),
      );
      if (hit !== undefined) {
        setActiveIndex(hit.index);
      }
    }
  };

  const activate = (item: ContextMenuItem): void => {
    if (item.disabled === true || item.separator === true) {
      return;
    }
    item.onSelect?.();
    onClose();
  };

  const style: React.CSSProperties = {
    position: 'fixed',
    left: Math.min(x, window.innerWidth - PANEL_W),
    top: Math.min(y, window.innerHeight - items.length * ITEM_ESTIMATE_H - 16),
    zIndex: 'var(--z-popover)',
  };

  return createPortal(
    <>
      {items
        .filter((item) => item.description !== undefined)
        .map((item) => (
          <span key={descriptionId(item)} id={descriptionId(item)} className="sr-only">
            {item.description}
          </span>
        ))}
      <div
        ref={ref}
        style={style}
        onContextMenu={(event) => event.preventDefault()}
        className="max-h-[70vh] min-w-[180px] overflow-y-auto rounded-[var(--radius-sm)] border border-border-subtle bg-surface-floating py-1 shadow-[var(--shadow-lg)]"
      >
        <div role="menu" aria-label="Context menu" tabIndex={-1} onKeyDown={onMenuKeyDown}>
          {items.map((item, index) => {
            if (item.separator) {
              return (
                <div
                  role="separator"
                  // eslint-disable-next-line react/no-array-index-key
                  key={`sep-${index}`}
                  className="my-1 border-t border-border-subtle"
                />
              );
            }
            const Icon = item.icon;
            const active = index === activeIndex;
            return (
              <div
                role="menuitem"
                key={item.label ?? `item-${index}`}
                data-menu-index={index}
                aria-disabled={item.disabled === true || undefined}
                aria-describedby={item.description !== undefined ? descriptionId(item) : undefined}
                tabIndex={active ? 0 : -1}
                onClick={() => activate(item)}
                onMouseEnter={() => {
                  if (item.disabled !== true) {
                    setActiveIndex(index);
                  }
                }}
                className={`flex min-h-8 w-full cursor-pointer items-center gap-2 px-3 text-left text-[13px] focus-visible:outline-none ${
                  item.disabled === true
                    ? 'cursor-default text-muted-text opacity-50'
                    : item.danger === true
                      ? `text-destructive ${active ? 'bg-hover' : ''}`
                      : `text-foreground ${active ? 'bg-hover' : ''}`
                }`}
              >
                {Icon !== undefined && <Icon size={14} />}
                <span className="flex-1 truncate" title={item.title}>
                  {item.label}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </>,
    document.body,
  );
}
