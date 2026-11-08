// SPDX-License-Identifier: GPL-3.0-only
// The plugin store: the registry rows every plugin surface reads (the
// composer's action row, the settings screen's sections). Pulled on demand by
// each surface's mount and refreshed after any verb; the registry's discovery
// rescan rides every pull.

import { create } from 'zustand';

import { pluginList, type PluginRow } from '@/services/plugins';

interface PluginStoreState {
  /** The last pulled registry rows; empty until the first pull. */
  rows: PluginRow[];
  /** Pulls the registry afresh. */
  refresh: () => Promise<void>;
}

export const usePluginStore = create<PluginStoreState>((set) => ({
  rows: [],

  refresh: async () => {
    try {
      set({ rows: await pluginList() });
    } catch (err) {
      // A refused pull leaves the standing rows; the next mount retries.
      console.error('plugins: the registry pull failed', err);
    }
  },
}));

/** The declared composer actions of every activated plugin, flattened. */
export function activatedActions(rows: PluginRow[]) {
  return rows
    .filter((row) => row.lifecycle === 'activated' && row.ui !== undefined)
    .flatMap((plugin) =>
      (plugin.ui?.composer_actions ?? []).map((action) => ({ plugin, action })),
    );
}

/** The declared settings sections of every activated plugin, flattened. */
export function activatedSections(rows: PluginRow[]) {
  return rows
    .filter((row) => row.lifecycle === 'activated' && row.ui !== undefined)
    .flatMap((plugin) =>
      (plugin.ui?.settings_sections ?? []).map((section) => ({ plugin, section })),
    );
}

/** The declared pages of every activated plugin, flattened. */
export function activatedPages(rows: PluginRow[]) {
  return rows
    .filter((row) => row.lifecycle === 'activated' && row.ui !== undefined)
    .flatMap((plugin) => (plugin.ui?.pages ?? []).map((page) => ({ plugin, page })));
}
