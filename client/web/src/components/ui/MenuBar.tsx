// SPDX-License-Identifier: GPL-3.0-only
// The application menus on the title bar: File, View, Tools, and Help, each
// a button with a dropdown panel. One menu stands open at a time, a hover
// over a neighbour switches across while one is open, Escape and a click
// outside close it, and every entry either dispatches an existing intent,
// calls a store setter, or talks to the window; nothing here is a
// placeholder. The list and theme entries draw as radios against the
// stores, the Density submenu's radios set the list store's density
// locally, the Fullscreen entry reads the window's real state at click
// time, and every window call is guarded so a plain browser quietly does
// nothing.

import { useEffect, useRef, useState } from 'react';

import { getCurrentWindow } from '@tauri-apps/api/window';

import { CheckIcon } from '@/components/icons';
import { DENSITIES } from '@/components/message-list/density';
import { openNewMessageComposer } from '@/features/composer/composeWindow';
import { useSettingsStore, type SettingsCategory } from '@/features/settings/settingsStore';
import { dispatch, intents } from '@/services/ipc';
import type { ViewMode } from '@/services/surfaces';
import { useAccountStore } from '@/stores/accountStore';
import { useListStore } from '@/stores/listStore';
import { useUiStore, type ThemeMode } from '@/stores/uiStore';

type MenuEntry =
  | { kind: 'item'; label: string; disabled?: boolean; onSelect: () => void }
  | { kind: 'radio'; label: string; checked: boolean; onSelect: () => void }
  | { kind: 'submenu'; label: string; entries: Array<{ kind: 'radio'; label: string; checked: boolean; onSelect: () => void }> }
  | { kind: 'separator' };

interface MenuSpec {
  label: string;
  entries: MenuEntry[];
}

/** Closes the Tauri window; outside a Tauri window the click does nothing. */
async function closeWindow(): Promise<void> {
  try {
    await getCurrentWindow().close();
  } catch {
    // Not in a Tauri window.
  }
}

/** Opens the pop-out composer window on a fresh message, the New message action's path. */
function newMail(): void {
  void openNewMessageComposer();
}

/** Opens the settings screen on one category. */
function openSettingsOn(category: SettingsCategory): void {
  useSettingsStore.getState().requestCategory(category);
  useUiStore.getState().setSettingsOpen(true);
}

const VIEW_MODES: ReadonlyArray<{ id: ViewMode; label: string }> = [
  { id: 'flat', label: 'Flat' },
  { id: 'threaded', label: 'Threaded' },
];

const THEME_MODES: ReadonlyArray<{ id: ThemeMode; label: string }> = [
  { id: 'follow_host', label: 'Follow host' },
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
];

/** One dropdown panel's entry. */
function MenuEntryRow({ entry, onDone }: { entry: MenuEntry; onDone: () => void }) {
  if (entry.kind === 'separator') {
    return <div role="separator" className="mx-2 my-1 h-px bg-border-subtle" />;
  }
  if (entry.kind === 'submenu') {
    return <SubmenuRow entry={entry} onDone={onDone} />;
  }
  const marked = entry.kind !== 'item';
  return (
    <button
      type="button"
      role={entry.kind === 'item' ? 'menuitem' : 'menuitemradio'}
      aria-checked={marked ? entry.checked : undefined}
      aria-disabled={entry.kind === 'item' ? entry.disabled : undefined}
      onClick={() => {
        if (entry.kind === 'item' && entry.disabled) {
          return;
        }
        entry.onSelect();
        onDone();
      }}
      className={`flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
        entry.kind === 'item' && entry.disabled
          ? 'cursor-default text-muted-foreground opacity-60'
          : 'text-foreground hover:bg-hover'
      }`}
    >
      <span aria-hidden="true" className="grid w-4 shrink-0 place-items-center text-primary">
        {marked && entry.checked ? <CheckIcon size={13} /> : null}
      </span>
      <span className="truncate">{entry.label}</span>
    </button>
  );
}

/** One nested submenu: opens on hover or click, its radios act and close
 * the whole menu through the parent's onDone. */
function SubmenuRow({
  entry,
  onDone,
}: {
  entry: Extract<MenuEntry, { kind: 'submenu' }>;
  onDone: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div
      className="relative"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        type="button"
        role="menuitem"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <span aria-hidden="true" className="grid w-4 shrink-0 place-items-center text-primary" />
        <span className="flex-1 truncate">{entry.label}</span>
        <span aria-hidden="true" className="text-muted-foreground">
          ▸
        </span>
      </button>
      {open && (
        <div
          role="menu"
          aria-label={entry.label}
          className="absolute left-full top-0 z-[var(--z-dropdown)] w-40 overflow-hidden rounded-[var(--radius-sm)] border border-border-subtle bg-surface-floating py-1 shadow-[var(--shadow-lg)]"
        >
          {entry.entries.map((sub) => (
            <MenuEntryRow key={sub.label} entry={sub} onDone={onDone} />
          ))}
        </div>
      )}
    </div>
  );
}

export function MenuBar() {
  const viewMode = useListStore((s) => s.viewMode);
  const density = useListStore((s) => s.density);
  const themeMode = useUiStore((s) => s.themeMode);
  const accountCount = useAccountStore((s) => s.accounts.length);
  const [openMenu, setOpenMenu] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  // Escape and a click outside close the standing menu.
  useEffect(() => {
    if (openMenu === null) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        setOpenMenu(null);
      }
    };
    const onPointerDown = (event: MouseEvent): void => {
      if (rootRef.current?.contains(event.target as Node) !== true) {
        setOpenMenu(null);
      }
    };
    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('mousedown', onPointerDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('mousedown', onPointerDown);
    };
  }, [openMenu]);

  const toggleFullscreen = async (): Promise<void> => {
    try {
      const win = getCurrentWindow();
      await win.setFullscreen(!(await win.isFullscreen()));
    } catch {
      // Not in a Tauri window.
    }
  };

  const menus: MenuSpec[] = [
    {
      label: 'File',
      entries: [
        {
          kind: 'item',
          label: 'New Mail',
          disabled: accountCount === 0,
          onSelect: newMail,
        },
        {
          kind: 'item',
          label: 'Add account',
          onSelect: () => useUiStore.getState().setAccountSetupOpen(true),
        },
        { kind: 'item', label: 'Sync now', onSelect: () => void dispatch(intents.refreshMail()) },
        {
          kind: 'item',
          label: 'Preferences',
          onSelect: () => useUiStore.getState().setSettingsOpen(true),
        },
        { kind: 'separator' },
        { kind: 'item', label: 'Close Window', onSelect: () => void closeWindow() },
      ],
    },
    {
      label: 'View',
      entries: [
        ...VIEW_MODES.map(
          ({ id, label }): MenuEntry => ({
            kind: 'radio',
            label,
            checked: viewMode === id,
            onSelect: () => void dispatch(intents.setViewMode(id)),
          }),
        ),
        { kind: 'separator' },
        {
          kind: 'submenu',
          label: 'Density',
          entries: DENSITIES.map(({ id, label }) => ({
            kind: 'radio' as const,
            label,
            checked: density === id,
            onSelect: () => useListStore.getState().setDensity(id),
          })),
        },
        { kind: 'separator' },
        { kind: 'item', label: 'Toggle Fullscreen', onSelect: () => void toggleFullscreen() },
      ],
    },
    {
      label: 'Tools',
      entries: THEME_MODES.map(
        ({ id, label }): MenuEntry => ({
          kind: 'radio',
          label,
          checked: themeMode === id,
          onSelect: () => void dispatch(intents.setTheme(id)),
        }),
      ),
    },
    {
      label: 'Help',
      entries: [
        { kind: 'item', label: 'About Astra Mail', onSelect: () => openSettingsOn('about') },
      ],
    },
  ];

  return (
    <div ref={rootRef} className="ml-1 flex items-center" aria-label="Application menus">
      {menus.map((menu) => {
        const open = openMenu === menu.label;
        return (
          <div key={menu.label} className="relative">
            <button
              type="button"
              aria-haspopup="menu"
              aria-expanded={open}
              onClick={() => setOpenMenu(open ? null : menu.label)}
              onMouseEnter={() => {
                if (openMenu !== null && !open) {
                  setOpenMenu(menu.label);
                }
              }}
              className={`h-7 rounded-[var(--radius-xs)] px-2 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                open ? 'bg-hover text-foreground' : 'text-muted-foreground hover:bg-hover hover:text-foreground'
              }`}
            >
              {menu.label}
            </button>
            {open && (
              <div
                role="menu"
                aria-label={menu.label}
                className="absolute left-0 top-full z-[var(--z-dropdown)] mt-0.5 w-48 overflow-hidden rounded-[var(--radius-sm)] border border-border-subtle bg-surface-floating py-1 shadow-[var(--shadow-lg)]"
              >
                {menu.entries.map((entry, index) => (
                  <MenuEntryRow
                    key={entry.kind === 'separator' ? `separator-${index}` : entry.label}
                    entry={entry}
                    onDone={() => setOpenMenu(null)}
                  />
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
