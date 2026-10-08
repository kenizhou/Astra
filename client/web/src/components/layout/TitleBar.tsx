// SPDX-License-Identifier: GPL-3.0-only
// The frameless title bar: the whole strip is a drag region and a double click
// on it toggles maximise, the pattern the tauri drag attribute enables. The
// product name, the theme toggle, and the account setup entry sit on it, each
// its own no-drag target by virtue of being the event's target rather than the
// region itself.

import { getCurrentWindow } from '@tauri-apps/api/window';

import { WindowTitleControls } from '@/components/ui/WindowTitleControls';
import { useUiStore } from '@/stores/uiStore';

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
      onDoubleClick={toggleMaximise}
      className="flex h-[var(--header-h)] shrink-0 select-none items-center border-b border-border bg-chrome pl-3 text-chrome-foreground"
    >
      <span data-tauri-drag-region className="text-[13px] font-semibold">
        Astra Mail
      </span>

      <div data-tauri-drag-region className="h-full min-w-[40px] flex-1" />

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
