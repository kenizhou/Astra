// SPDX-License-Identifier: GPL-3.0-only
// The plugin settings section: one declared section rendered as a settings
// category, each field in its kind's control. Every change is a
// custom-namespace call into the worker (`plugin.<id>.set_setting`), so the
// plugin owns its own persistence in its private data directory; the chrome
// only renders and forwards.

import { useEffect } from 'react';

import { pluginCall, type PluginRow, type SettingsSectionDescriptor } from '@/services/plugins';
import { activatedSections, usePluginStore } from '@/stores/pluginStore';

/** One field in its kind's control. */
function Field({
  plugin,
  field,
}: {
  plugin: PluginRow;
  field: SettingsSectionDescriptor['fields'][number];
}) {
  const set = (value: unknown) =>
    void pluginCall(plugin.id, `plugin.${plugin.id}.set_setting`, { key: field.key, value }).catch(
      () => undefined,
    );
  const inputClass =
    'rounded-full border border-input bg-transparent px-3 py-1 text-xs focus-visible:outline-none focus-visible:border-primary';
  switch (field.kind) {
    case 'toggle':
      return (
        <label className="flex items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            aria-label={field.label}
            onChange={(event) => set(event.target.checked)}
          />
          {field.label}
        </label>
      );
    case 'select':
      return (
        <label className="flex items-center gap-2 text-xs text-foreground">
          {field.label}
          <select
            aria-label={field.label}
            className={inputClass}
            onChange={(event) => set(event.target.value)}
          >
            {field.options.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
        </label>
      );
    case 'password':
      return (
        <label className="flex items-center gap-2 text-xs text-foreground">
          {field.label}
          <input
            type="password"
            aria-label={field.label}
            className={inputClass}
            onChange={(event) => set(event.target.value)}
          />
        </label>
      );
    case 'action':
      return (
        <button
          type="button"
          onClick={() => set(null)}
          className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {field.label}
        </button>
      );
    default:
      return (
        <label className="flex items-center gap-2 text-xs text-foreground">
          {field.label}
          <input
            type="text"
            aria-label={field.label}
            className={inputClass}
            onChange={(event) => set(event.target.value)}
          />
        </label>
      );
  }
}

/** One declared section, with its title and fields. */
export function PluginSettingsSection({
  plugin,
  section,
}: {
  plugin: PluginRow;
  section: SettingsSectionDescriptor;
}) {
  return (
    <section aria-label={section.title} className="space-y-2">
      <h3 className="text-xs font-medium text-muted-foreground">{section.title}</h3>
      {section.fields.map((field) => (
        <Field key={field.key} plugin={plugin} field={field} />
      ))}
    </section>
  );
}

/** Every activated plugin's declared sections, rendered in order. */
export function PluginSettingsSections() {
  const rows = usePluginStore((s) => s.rows);
  const refresh = usePluginStore((s) => s.refresh);
  useEffect(() => {
    if (rows.length === 0) {
      void refresh();
    }
  }, [rows.length, refresh]);
  return (
    <>
      {activatedSections(rows).map(({ plugin, section }) => (
        <PluginSettingsSection key={`${plugin.id}:${section.id}`} plugin={plugin} section={section} />
      ))}
    </>
  );
}
