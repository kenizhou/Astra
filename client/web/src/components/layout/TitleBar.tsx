// SPDX-License-Identifier: GPL-3.0-only
// The frameless title bar: the whole strip is a drag region, and a double
// click on one of the dedicated drag regions toggles maximise. The handler
// lives on the regions alone, never on the header: a double click on a button
// bubbles to the header, and an answer there would net the control an extra
// toggle on top of its own clicks. The product name, the shell's view
// switch, the search box, the theme toggle, and the account setup entry sit
// on the strip as their own no-drag targets by virtue of being the event's
// target rather than the region itself. The search box holds the strip's
// centre between two drag regions, the scope dropdown riding inside it.

import { useEffect, useRef, useState } from 'react';

import { getCurrentWindow } from '@tauri-apps/api/window';

import { WindowTitleControls } from '@/components/ui/WindowTitleControls';
import { dispatch, intents } from '@/services/ipc';
import { useListStore } from '@/stores/listStore';
import { useUiStore } from '@/stores/uiStore';
import type { SearchScope } from '@/services/surfaces';
import type { ShellView } from '@/stores/uiStore';

/** The shell's views the switch offers; Contacts joins this list later. */
const SHELL_VIEWS: ReadonlyArray<{ id: ShellView; label: string }> = [
  { id: 'mail', label: 'Mail' },
  { id: 'calendar', label: 'Calendar' },
];

/** The shell's view switch: which feature fills the content row. */
export function ShellSwitch() {
  const shellView = useUiStore((s) => s.shellView);
  const setShellView = useUiStore((s) => s.setShellView);

  return (
    <div
      role="group"
      aria-label="View"
      className="ml-2 flex overflow-hidden rounded-[var(--radius)] border border-border"
    >
      {SHELL_VIEWS.map(({ id, label }) => (
        <button
          key={id}
          type="button"
          aria-pressed={shellView === id}
          onClick={() => setShellView(id)}
          className={`px-2 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
            id === 'mail' ? '' : 'border-l border-border'
          } ${
            shellView === id
              ? 'bg-selected font-medium text-selected-text'
              : 'text-muted-foreground hover:bg-hover hover:text-foreground'
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

/** How long the search box sits quiet before its query crosses. */
export const SEARCH_DEBOUNCE_MS = 500;

/** The scopes the dropdown offers, in the wire's own order. */
const SCOPES: ReadonlyArray<{ id: SearchScope; label: string }> = [
  { id: 'all_except_trash', label: 'All except trash' },
  { id: 'current_folder', label: 'Current folder' },
  { id: 'current_account', label: 'Current account' },
];

/**
 * The search box: typing debounces the query across so the list is asked
 * once per pause rather than once per keystroke, Escape clears at once and
 * disarms whatever pause was pending, and the scope dropdown narrows the
 * live search without waiting. The field echoes the query the snapshot's
 * search state carries, so a search that landed or exited elsewhere shows
 * here truthfully; the echo never arms the debounce itself.
 */
export function SearchBox() {
  const echo = useListStore((s) => s.search?.query ?? null);
  const scope = useListStore((s) => s.search?.scope ?? 'all_except_trash');
  const [text, setText] = useState(echo ?? '');
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const query = useRef(text);

  useEffect(() => {
    setText(echo ?? '');
  }, [echo]);

  // A pause the box armed must never outlive the box.
  useEffect(() => {
    return () => {
      if (timer.current !== null) {
        clearTimeout(timer.current);
        timer.current = null;
      }
    };
  }, []);

  const disarm = (): void => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  };

  const onQueryChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    query.current = event.target.value;
    setText(event.target.value);
    disarm();
    timer.current = setTimeout(() => {
      timer.current = null;
      void dispatch(intents.search(query.current));
    }, SEARCH_DEBOUNCE_MS);
  };

  const onEscape = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== 'Escape') {
      return;
    }
    disarm();
    query.current = '';
    setText('');
    void dispatch(intents.search(null));
  };

  const onScopeChange = (event: React.ChangeEvent<HTMLSelectElement>) => {
    void dispatch(intents.setSearchScope(event.target.value as SearchScope));
  };

  return (
    <div className="flex h-6 w-full max-w-[360px] items-center gap-1 rounded-[var(--radius)] border border-border bg-surface pl-1 pr-2 focus-within:ring-2 focus-within:ring-ring">
      <select
        aria-label="Search scope"
        title="Search scope"
        value={scope}
        onChange={onScopeChange}
        className="max-w-[7.5rem] shrink-0 cursor-pointer truncate bg-transparent text-xs text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:text-foreground"
      >
        {SCOPES.map(({ id, label }) => (
          <option key={id} value={id}>
            {label}
          </option>
        ))}
      </select>
      <span aria-hidden="true" className="h-4 w-px shrink-0 bg-border" />
      <input
        type="text"
        aria-label="Search mail"
        placeholder="Search mail"
        value={text}
        onChange={onQueryChange}
        onKeyDown={onEscape}
        className="h-full min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground"
      />
    </div>
  );
}

export function TitleBar() {
  const themeMode = useUiStore((s) => s.themeMode);
  const toggleTheme = useUiStore((s) => s.toggleTheme);
  const openSetup = () => useUiStore.getState().setAccountSetupOpen(true);

  const toggleMaximise = async () => {
    try {
      await getCurrentWindow().toggleMaximize();
    } catch {
      // Not in a Tauri window.
    }
  };

  return (
    <header
      data-tauri-drag-region
      className="flex h-[var(--header-h)] shrink-0 select-none items-center border-b border-border bg-chrome pl-3 text-chrome-foreground"
    >
      <span
        data-tauri-drag-region
        onDoubleClick={toggleMaximise}
        className="text-[13px] font-semibold"
      >
        Astra Mail
      </span>

      <ShellSwitch />

      <div
        data-tauri-drag-region
        onDoubleClick={toggleMaximise}
        className="h-full min-w-[40px] flex-1"
      />

      <SearchBox />

      <div
        data-tauri-drag-region
        onDoubleClick={toggleMaximise}
        className="h-full min-w-[40px] flex-1"
      />

      <button
        type="button"
        onClick={openSetup}
        title="Add an account"
        className="mr-1 rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        Add account
      </button>
      <button
        type="button"
        onClick={toggleTheme}
        title={themeMode === 'light' ? 'Switch to dark' : 'Switch to light'}
        aria-label="Toggle theme"
        className="mr-1 rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {themeMode === 'light' ? 'Dark' : 'Light'}
      </button>

      <WindowTitleControls />
    </header>
  );
}
