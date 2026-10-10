// SPDX-License-Identifier: GPL-3.0-only
// The shell's chrome state: the theme mode the title bar toggles, the engine's
// lifecycle as the shell sees it, the sync states the status bar draws, and
// whether the account setup flow or the outbox panel is up.

import { create } from 'zustand';

import type { AccountSyncProgress, SyncProgressSnapshot } from '@/services/surfaces';

export type ThemeMode = 'light' | 'dark';

/** The shell's view of the app's lifecycle, until the first pull lands. */
export type EngineState = 'starting' | 'running';

interface UiStoreState {
  themeMode: ThemeMode;
  engineState: EngineState;
  /** Per-account sync positions from the last sync-progress pull. */
  sync: AccountSyncProgress[];
  accountSetupOpen: boolean;
  /** Whether the outbox panel is up over the shell's corner. */
  outboxOpen: boolean;
  /** Whether the agent panel is up over the shell's corner. */
  agentOpen: boolean;
  setTheme: (mode: ThemeMode) => void;
  toggleTheme: () => void;
  markRunning: () => void;
  applySyncProgress: (snapshot: SyncProgressSnapshot) => void;
  setAccountSetupOpen: (open: boolean) => void;
  setOutboxOpen: (open: boolean) => void;
  setAgentOpen: (open: boolean) => void;
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
  outboxOpen: false,
  agentOpen: false,

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

  setOutboxOpen: (open) => set({ outboxOpen: open }),

  setAgentOpen: (open) => set({ agentOpen: open }),
}));
