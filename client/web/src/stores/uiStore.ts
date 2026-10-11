// SPDX-License-Identifier: GPL-3.0-only
// The shell's chrome state: the theme mode the title bar toggles, the engine's
// lifecycle as the shell sees it, the sync states the status bar draws, and
// whether the account setup flow, the outbox panel, the agent panel, or the
// AI documents panel is up.

import { create } from 'zustand';

import type { AccountSyncProgress, SyncProgressSnapshot } from '@/services/surfaces';

export type ThemeMode = 'light' | 'dark';

/** The shell's view of the app's lifecycle, until the first pull lands. */
export type EngineState = 'starting' | 'running';

/**
 * The shell's main view: which feature fills the content row. Mail is the
 * default; the list is the one place a view is added, so Contacts extends it.
 */
export type ShellView = 'mail' | 'calendar';

interface UiStoreState {
  themeMode: ThemeMode;
  engineState: EngineState;
  /** Per-account sync positions from the last sync-progress pull. */
  sync: AccountSyncProgress[];
  accountSetupOpen: boolean;
  /** The feature filling the shell's content row. */
  shellView: ShellView;
  /** Whether the outbox panel is up over the shell's corner. */
  outboxOpen: boolean;
  /** Whether the agent panel is up over the shell's corner. */
  agentOpen: boolean;
  /** Whether the AI documents panel is up over the shell's corner. */
  aiPanelOpen: boolean;
  setTheme: (mode: ThemeMode) => void;
  toggleTheme: () => void;
  markRunning: () => void;
  applySyncProgress: (snapshot: SyncProgressSnapshot) => void;
  setAccountSetupOpen: (open: boolean) => void;
  setShellView: (view: ShellView) => void;
  setOutboxOpen: (open: boolean) => void;
  setAgentOpen: (open: boolean) => void;
  setAiPanelOpen: (open: boolean) => void;
}

/** Applies the theme classes on the document root; the tokens live on them. */
function applyThemeClass(mode: ThemeMode): void {
  const classes = document.documentElement.classList;
  classes.toggle('light', mode === 'light');
  classes.toggle('dark', mode === 'dark');
}

export const useUiStore = create<UiStoreState>((set) => ({
  themeMode: 'light',
  engineState: 'starting',
  sync: [],
  accountSetupOpen: false,
  shellView: 'mail',
  outboxOpen: false,
  agentOpen: false,
  aiPanelOpen: false,

  setTheme: (mode) => {
    applyThemeClass(mode);
    set({ themeMode: mode });
  },

  toggleTheme: () => {
    const mode: ThemeMode = useUiStore.getState().themeMode === 'light' ? 'dark' : 'light';
    applyThemeClass(mode);
    set({ themeMode: mode });
  },

  markRunning: () => set({ engineState: 'running' }),

  applySyncProgress: (snapshot) => set({ sync: snapshot.accounts }),

  setAccountSetupOpen: (open) => set({ accountSetupOpen: open }),

  setShellView: (view) => set({ shellView: view }),

  setOutboxOpen: (open) => set({ outboxOpen: open }),

  setAgentOpen: (open) => set({ agentOpen: open }),

  setAiPanelOpen: (open) => set({ aiPanelOpen: open }),
}));
