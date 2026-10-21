// SPDX-License-Identifier: GPL-3.0-only
// The settings surface's state: the last pulled snapshot's blocks (the
// persisted settings, the account rows, the about block, the newest
// write error) held verbatim for the screen to render. The theme the
// block carries routes onward into the shell's own chrome state, so the
// persisted block is the one truth the whole shell paints from. The
// revision moves on every apply, so the editors that watch a write's
// settle can tell a fresh publish from the one they dispatched against.

import { create } from 'zustand';

import type {
  AboutBlock,
  AccountSettingsRow,
  SettingsBlock,
  SettingsError,
  SettingsSnapshot,
} from '@/services/ipc';
import { useUiStore } from '@/stores/uiStore';

/** The category id, one per detail the settings screen ships. */
export type SettingsCategory =
  | 'general'
  | 'calendar'
  | 'reading'
  | 'composing'
  | 'shortcuts'
  | 'accounts'
  | 'plugins'
  | 'advanced'
  | 'about';

/** The block the core hands a fresh preferences document: the shipped defaults. */
const DEFAULT_SETTINGS: SettingsBlock = {
  theme: 'follow_host',
  twenty_four_clock: false,
  zone: 'Etc/UTC',
  first_day_of_week: 'mo',
  horizon_days: 42,
  quote_below: true,
  view_mode: 'flat',
};

interface SettingsStoreState {
  /** The persisted settings block, the whole of it. */
  settings: SettingsBlock;
  /** One row per configured account, as the accounts page renders them. */
  accounts: AccountSettingsRow[];
  /** The version and the attributions, the core's answer. */
  about: AboutBlock;
  /** Why the newest settings write refused or failed, when it did. */
  error: SettingsError | undefined;
  /** Moves on every applied snapshot, so watches see a fresh publish. */
  revision: number;
  /** The category the next open lands on; null opens on the last one. */
  requestedCategory: SettingsCategory | null;
  /** Applies a pulled snapshot. */
  applySnapshot: (snapshot: SettingsSnapshot) => void;
  /** Asks the next open to land on one category, or clears the ask. */
  requestCategory: (category: SettingsCategory | null) => void;
}

export const useSettingsStore = create<SettingsStoreState>((set) => ({
  settings: DEFAULT_SETTINGS,
  accounts: [],
  about: { version: '' },
  error: undefined,
  revision: 0,
  requestedCategory: null,

  applySnapshot: (snapshot) => {
    useUiStore.getState().setTheme(snapshot.settings.theme);
    set((state) => ({
      settings: snapshot.settings,
      accounts: snapshot.accounts ?? [],
      about: snapshot.about,
      error: snapshot.error,
      revision: state.revision + 1,
    }));
  },

  requestCategory: (category) => set({ requestedCategory: category }),
}));
