// SPDX-License-Identifier: GPL-3.0-only
// The frameless title bar: the whole strip is a drag region, and a double
// click on one of the dedicated drag regions toggles maximise. The handler
// lives on the regions alone, never on the header: a double click on a button
// bubbles to the header, and an answer there would net the control an extra
// toggle on top of its own clicks. The product name, the ambient island, the
// theme override cycle, the account setup entry, and the settings gear sit
// on the strip as their own no-drag targets by virtue of being the event's
// target rather than the region itself. The island holds the strip's centre
// between two drag regions. The strip wears the glass treatment with no
// underline: the ribbon's own top edge carries the separation.

import { getCurrentWindow } from '@tauri-apps/api/window';

import { MoonIcon, PlusIcon, SettingsIcon, SunIcon } from '@/components/icons';
import { WindowTitleControls } from '@/components/ui/WindowTitleControls';
import { dispatch, intents } from '@/services/ipc';
import { nextThemeMode, useUiStore } from '@/stores/uiStore';

import { AmbientIsland } from './AmbientIsland';

// The island owns the debounce; the people pane reads it from here.
export { SEARCH_DEBOUNCE_MS } from './AmbientIsland';

/** The title bar's icon button shape. */
const TITLE_ICON_CLASS =
  'mr-1 grid h-7 w-7 place-items-center rounded-full text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

export function TitleBar() {
  const themeMode = useUiStore((s) => s.themeMode);
  const openSetup = () => useUiStore.getState().setAccountSetupOpen(true);
  const openSettings = () => useUiStore.getState().setSettingsOpen(true);
  const cycleTheme = () => {
    // The override lands only through the republished settings block, so
    // the click dispatches and the snapshot repaints; nothing applies here.
    void dispatch(intents.setTheme(nextThemeMode(themeMode)));
  };
  const next = nextThemeMode(themeMode);

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
      className="glass relative flex h-[var(--header-h)] shrink-0 select-none items-center pl-3 text-chrome-foreground"
    >
      <span
        data-tauri-drag-region
        onDoubleClick={toggleMaximise}
        className="text-[13px] font-semibold"
      >
        Astra Mail
      </span>

      <div
        data-tauri-drag-region
        onDoubleClick={toggleMaximise}
        className="h-full min-w-[40px] flex-1"
      />

      <AmbientIsland />

      <div
        data-tauri-drag-region
        onDoubleClick={toggleMaximise}
        className="h-full min-w-[40px] flex-1"
      />

      <button
        type="button"
        onClick={openSetup}
        aria-label="Add account"
        title="Add an account"
        className={TITLE_ICON_CLASS}
      >
        <PlusIcon size={16} />
      </button>
      <button
        type="button"
        onClick={openSettings}
        title="Settings"
        aria-label="Settings"
        className={TITLE_ICON_CLASS}
      >
        <SettingsIcon size={16} />
      </button>
      <button
        type="button"
        onClick={cycleTheme}
        title={
          themeMode === 'follow_host'
            ? 'Following the host; switch to light'
            : `Switch to ${next === 'follow_host' ? 'following the host' : next}`
        }
        aria-label="Toggle theme"
        className={TITLE_ICON_CLASS}
      >
        {themeMode === 'follow_host' ? (
          <span className="text-[11px] font-medium">Auto</span>
        ) : themeMode === 'light' ? (
          <SunIcon size={16} />
        ) : (
          <MoonIcon size={16} />
        )}
      </button>

      <WindowTitleControls />
    </header>
  );
}
