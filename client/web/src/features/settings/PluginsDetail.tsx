// SPDX-License-Identifier: GPL-3.0-only
// The Plugins settings detail: every discovered plugin with its lifecycle,
// its granted permissions, and the enable/disable verb, with each activated
// plugin's declared settings sections below. The registry rescans on every
// pull, so a plugin directory dropped in while the app runs appears here
// without a restart; grant edits apply at the worker's next start.

import { useEffect } from 'react';

import { pluginDisable, pluginEnable, type PluginRow } from '@/services/plugins';
import { PluginSettingsSections } from '@/features/plugins/PluginSettingsSection';
import { usePluginStore } from '@/stores/pluginStore';

/** One plugin's row: its name, state, grants, and the verb its state allows. */
function PluginRowView({ row }: { row: PluginRow }) {
  const refresh = usePluginStore((s) => s.refresh);
  const active = row.lifecycle === 'activated';
  const failed = row.lifecycle === 'failed';
  const toggle = async () => {
    if (active) {
      await pluginDisable(row.id);
    } else if (!failed) {
      await pluginEnable(row.id).catch(() => undefined);
    }
    await refresh();
  };
  return (
    <div className="flex items-center justify-between gap-2 rounded-[var(--radius-xs)] border border-border-subtle px-3 py-2">
      <div className="min-w-0">
        <p className="truncate text-xs font-medium text-foreground">{row.name}</p>
        <p className="truncate text-[11px] text-muted-foreground">
          {row.id} · {row.lifecycle}
          {row.grants.length > 0 ? ` · ${row.grants.join(', ')}` : ''}
        </p>
      </div>
      <button
        type="button"
        disabled={failed}
        onClick={() => void toggle()}
        className="shrink-0 rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
      >
        {active ? 'Disable' : 'Enable'}
      </button>
    </div>
  );
}

/** The plugins detail: the registry rows, then the declared sections. */
export function PluginsDetail() {
  const rows = usePluginStore((s) => s.rows);
  const refresh = usePluginStore((s) => s.refresh);
  useEffect(() => {
    void refresh();
  }, [refresh]);
  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <h3 className="text-xs font-medium text-muted-foreground">Plugins</h3>
        {rows.length === 0 ? (
          <p className="text-xs text-muted-foreground">No plugins discovered.</p>
        ) : (
          rows.map((row) => <PluginRowView key={row.id} row={row} />)
        )}
      </div>
      <PluginSettingsSections />
    </div>
  );
}
