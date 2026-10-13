// SPDX-License-Identifier: GPL-3.0-only
// The shell's chrome state: the theme mode the title bar cycles (following
// the host by default, Light and Dark as explicit overrides, the host
// preference followed live while follow-host stands), the engine's
// lifecycle as the shell sees it, the sync states the status bar draws,
// and whether the account setup flow, the settings screen, the agent
// panel, the AI documents panel, or the compact shell's folder drawer is
// up.

import { create } from 'zustand';

import type { AccountSyncProgress, SyncProgressSnapshot } from '@/services/surfaces';

export type ThemeMode = 'follow_host' | 'light' | 'dark';

/** The next mode the override cycle steps to: host, light, dark, and back. */
export function nextThemeMode(mode: ThemeMode): ThemeMode {
  if (mode === 'follow_host') {
    return 'light';
  }
  if (mode === 'light') {
    return 'dark';
  }
  return 'follow_host';
}

/** The shell's view of the app's lifecycle, until the first pull lands. */
export type EngineState = 'starting' | 'running';

/**
 * The shell's main view: which feature fills the content row. Mail is the
 * default; the list is the one place a view is added, so People joins Mail
 * and Calendar here.
 */
export type ShellView = 'mail' | 'calendar' | 'people';

interface UiStoreState {
  themeMode: ThemeMode;
  engineState: EngineState;
  /** Per-account sync positions from the last sync-progress pull. */
  sync: AccountSyncProgress[];
  accountSetupOpen: boolean;
  /** The feature filling the shell's content row. */
  shellView: ShellView;
  /** Whether the agent panel is up over the shell's corner. */
  agentOpen: boolean;
  /** Whether the AI documents panel is up over the shell's corner. */
  aiPanelOpen: boolean;
  /** Whether the settings screen rides above the shell. */
  settingsOpen: boolean;
  /** Whether the compact shell's folder drawer is up over the content. */
  folderDrawerOpen: boolean;
  setTheme: (mode: ThemeMode) => void;
  markRunning: () => void;
  applySyncProgress: (snapshot: SyncProgressSnapshot) => void;
  setAccountSetupOpen: (open: boolean) => void;
  setShellView: (view: ShellView) => void;
  setAgentOpen: (open: boolean) => void;
  setAiPanelOpen: (open: boolean) => void;
  setSettingsOpen: (open: boolean) => void;
  setFolderDrawerOpen: (open: boolean) => void;
}

/** The host's own colour scheme, when the platform reports one. */
function hostPrefersDark(): boolean {
  if (typeof window === 'undefined' || window.matchMedia === undefined) {
    return false;
  }
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

/** Tears the live host listener down; null while none stands. */
let detachHost: (() => void) | null = null;

/**
 * Keeps follow-host live: while the mode stands, one listener follows the
 * host preference and repaints; any other mode detaches it. The listener
 * is re-armed on every follow-host apply, so a query the platform handed
 * back anew is always the one listened on. The listener outlives the
 * components (the store owns the theme for the whole session), so it is
 * swapped here rather than in an effect.
 */
function syncHostListener(mode: ThemeMode): void {
  detachHost?.();
  detachHost = null;
  if (mode !== 'follow_host') {
    return;
  }
  if (typeof window === 'undefined' || window.matchMedia === undefined) {
    return;
  }
  const query = window.matchMedia('(prefers-color-scheme: dark)');
  const onChange = (): void => {
    if (useUiStore.getState().themeMode === 'follow_host') {
      applyThemeClass('follow_host');
    }
  };
  query.addEventListener('change', onChange);
  detachHost = (): void => query.removeEventListener('change', onChange);
}

/**
 * Applies the theme classes on the document root; the tokens live on them.
 * Follow-host resolves through the host preference at apply time.
 */
function applyThemeClass(mode: ThemeMode): void {
  const resolved: 'light' | 'dark' =
    mode === 'follow_host' ? (hostPrefersDark() ? 'dark' : 'light') : mode;
  const classes = document.documentElement.classList;
  classes.toggle('light', resolved === 'light');
  classes.toggle('dark', resolved === 'dark');
}

export const useUiStore = create<UiStoreState>((set) => ({
  themeMode: 'follow_host',
  engineState: 'starting',
  sync: [],
  accountSetupOpen: false,
  shellView: 'mail',
  agentOpen: false,
  aiPanelOpen: false,
  settingsOpen: false,
  folderDrawerOpen: false,

  setTheme: (mode) => {
    syncHostListener(mode);
    applyThemeClass(mode);
    set({ themeMode: mode });
  },

  markRunning: () => set({ engineState: 'running' }),

  applySyncProgress: (snapshot) => set({ sync: snapshot.accounts }),

  setAccountSetupOpen: (open) => set({ accountSetupOpen: open }),

  setShellView: (view) => set({ shellView: view }),

  setAgentOpen: (open) => set({ agentOpen: open }),

  setAiPanelOpen: (open) => set({ aiPanelOpen: open }),

  setSettingsOpen: (open) => set({ settingsOpen: open }),

  setFolderDrawerOpen: (open) => set({ folderDrawerOpen: open }),
}));
