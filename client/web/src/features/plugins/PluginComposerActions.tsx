// SPDX-License-Identifier: GPL-3.0-only
// The plugin composer actions: one button per declared action of every
// activated plugin, standing in the composer chrome beside the host's own.
// A click is a custom-namespace call into the worker
// (`plugin.<id>.<action>`), and the worker's answer may carry the action's
// new pressed state, which is how a toggle shows its effect without the
// plugin ever touching the draft.

import { useEffect, useState } from 'react';

import { pluginCall, type PluginRow, type ComposerActionDescriptor } from '@/services/plugins';
import { activatedActions, usePluginStore } from '@/stores/pluginStore';
import { useComposerStore } from '@/features/composer/composerStore';

/** One declared action's button. */
function ActionButton({
  plugin,
  action,
}: {
  plugin: PluginRow;
  action: ComposerActionDescriptor;
}) {
  const composition = useComposerStore((s) => s.composition);
  const [pressed, setPressed] = useState(false);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    setBusy(true);
    try {
      const result = (await pluginCall(plugin.id, `plugin.${plugin.id}.${action.id}`, {
        composition,
      })) as { pressed?: boolean } | null;
      if (result && typeof result.pressed === 'boolean') {
        setPressed(result.pressed);
      }
    } catch {
      // A refused or dead worker answers nothing: the button simply stays.
    } finally {
      setBusy(false);
    }
  };

  return (
    <button
      type="button"
      aria-pressed={pressed}
      disabled={busy}
      onClick={() => void run()}
      className={`rounded-[var(--radius-xs)] px-2 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)] ${
        pressed
          ? 'bg-selected text-foreground'
          : 'text-muted-foreground hover:bg-hover hover:text-foreground'
      }`}
    >
      {action.label}
    </button>
  );
}

/** The actions strip: nothing renders when no activated plugin declares one. */
export function PluginComposerActions() {
  const rows = usePluginStore((s) => s.rows);
  const refresh = usePluginStore((s) => s.refresh);
  useEffect(() => {
    if (rows.length === 0) {
      void refresh();
    }
  }, [rows.length, refresh]);
  const actions = activatedActions(rows);
  if (actions.length === 0) {
    return null;
  }
  return (
    <>
      {actions.map(({ plugin, action }) => (
        <ActionButton key={`${plugin.id}:${action.id}`} plugin={plugin} action={action} />
      ))}
    </>
  );
}
