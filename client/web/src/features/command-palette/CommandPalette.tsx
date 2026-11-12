// SPDX-License-Identifier: GPL-3.0-only
// The command palette: a centred overlay holding a text input over a flat list
// of the shell's commands. The filter is a case-insensitive substring on the
// label (no fuzzy scoring), the arrows wrap, Enter runs the selected command
// and closes, Escape and the backdrop close, and the selection clamps against
// a shrinking list. The surface mounts fresh on every open, so the query and
// selection start clean each time. A disabled command shows its reason on the
// row and never runs; the focus moves into the field on open and back on close.

import { useEffect, useRef, useState } from 'react';

import { useDialogFocus } from '@/components/ui/useDialogFocus';
import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { useUiStore } from '@/stores/uiStore';

import { buildCommands, type PaletteCommand } from './commands';
import { closeCommandPalette, useCommandPaletteStore } from './useCommandPalette';

/** Case-insensitive substring match on the command label. */
function matchesQuery(command: PaletteCommand, query: string): boolean {
  const trimmed = query.trim().toLowerCase();
  return trimmed === '' || command.label.toLowerCase().includes(trimmed);
}

/** The overlay, mounted in the shell's tail; null while the store holds it closed. */
export function CommandPalette() {
  const isOpen = useCommandPaletteStore((state) => state.isOpen);
  if (!isOpen) {
    return null;
  }
  return <PaletteSurface />;
}

/** The open surface: a fresh mount per open resets the query and selection. */
function PaletteSurface() {
  // Subscribe to every store the registry reads, so a gate moves live while
  // the palette stands open over a background pull.
  useUiStore();
  useFolderStore();
  useListStore();
  useReadingStore();
  useAccountStore();

  const commands = buildCommands();
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useDialogFocus<HTMLInputElement>();
  const listRef = useRef<HTMLUListElement>(null);

  const filtered = commands.filter((command) => matchesQuery(command, query));
  // Clamp so a stale index can never point past the shrunken list.
  const activeIndex = Math.min(selectedIndex, Math.max(filtered.length - 1, 0));
  const active = filtered[activeIndex];

  // Keep the highlighted row in view while arrowing; jsdom has no scrollIntoView.
  useEffect(() => {
    listRef.current?.children[activeIndex]?.scrollIntoView?.({ block: 'nearest' });
  }, [activeIndex]);

  const run = (command: PaletteCommand): void => {
    if (command.disabled !== undefined) {
      return;
    }
    command.run();
    closeCommandPalette();
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>): void => {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setSelectedIndex(filtered.length === 0 ? 0 : (activeIndex + 1) % filtered.length);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setSelectedIndex(
        filtered.length === 0 ? 0 : (activeIndex - 1 + filtered.length) % filtered.length,
      );
    } else if (event.key === 'Enter') {
      if (active === undefined) {
        return;
      }
      event.preventDefault();
      run(active);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      closeCommandPalette();
    }
  };

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-start justify-center bg-[var(--backdrop)] p-4 pt-[15vh]"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          closeCommandPalette();
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        className="palette-pop-in w-full max-w-xl overflow-hidden rounded-xl border border-border-subtle bg-surface-floating shadow-[var(--shadow-lg)]"
      >
        <input
          ref={inputRef}
          type="text"
          aria-label="Search commands"
          aria-controls="command-palette-list"
          aria-activedescendant={active !== undefined ? `command-option-${active.id}` : undefined}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setSelectedIndex(0);
          }}
          onKeyDown={onKeyDown}
          placeholder="Type a command"
          className="w-full border-b border-border-subtle bg-transparent px-4 py-3 text-sm text-foreground outline-none placeholder:text-muted-foreground"
        />
        {filtered.length === 0 ? (
          <div
            className="px-4 py-6 text-sm text-muted-text"
            data-testid="command-palette-empty"
          >
            No matching commands.
          </div>
        ) : (
          <ul
            ref={listRef}
            id="command-palette-list"
            role="listbox"
            aria-label="Commands"
            className="scrollbar-none max-h-[50vh] overflow-y-auto py-1"
          >
            {filtered.map((command, index) => {
              const disabled = command.disabled !== undefined;
              const Icon = command.icon;
              const reasonId = `command-reason-${command.id}`;
              return (
                <li
                  key={command.id}
                  id={`command-option-${command.id}`}
                  role="option"
                  aria-selected={index === activeIndex}
                  aria-disabled={disabled || undefined}
                  aria-describedby={disabled ? reasonId : undefined}
                  title={command.disabled}
                  onMouseEnter={() => setSelectedIndex(index)}
                  onClick={() => run(command)}
                  className={`flex cursor-pointer items-center gap-3 px-4 py-2 text-left text-[13px] ${
                    disabled
                      ? 'cursor-default text-muted-text opacity-60'
                      : `text-foreground ${index === activeIndex ? 'bg-hover' : ''}`
                  }`}
                >
                  <span
                    aria-hidden="true"
                    className="flex w-4 shrink-0 items-center justify-center text-muted-text"
                  >
                    <Icon size={15} />
                  </span>
                  <span className="min-w-0 flex-1 truncate">{command.label}</span>
                  {disabled ? (
                    <span id={reasonId} className="shrink-0 text-[11px] text-muted-text">
                      {command.disabled}
                    </span>
                  ) : (
                    <span aria-hidden="true" className="shrink-0 text-[11px] text-muted-text">
                      {command.category}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}
