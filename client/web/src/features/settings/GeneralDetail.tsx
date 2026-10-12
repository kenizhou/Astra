// SPDX-License-Identifier: GPL-3.0-only
// The General detail: the theme picker (follow host, Light, Dark) and the
// clock picker (24 or 12 hour). Each control dispatches its intent and
// reflects the persisted block, never local optimism, and the persist
// failure's sentence renders under the controls it belongs to.

import { dispatch, intents } from '@/services/ipc';

import { useSettingsStore } from './settingsStore';

/** The theme choices, in the taxonomy's own order. */
const THEMES: ReadonlyArray<{ id: 'follow_host' | 'light' | 'dark'; label: string }> = [
  { id: 'follow_host', label: 'Follow host' },
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
];

/** The sentence a refused persist draws. */
export const PERSIST_SENTENCE =
  'The change could not be saved. Try again.';

/** The select chrome every picker in the detail shares. */
const fieldClass =
  'h-7 w-full rounded-[var(--radius-xs)] border border-input bg-background px-2 text-xs text-foreground outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring';

/** One labelled picker row. */
function Picker({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: ReadonlyArray<{ id: string; label: string }>;
  onChange: (id: string) => void;
}) {
  return (
    <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
      {label}
      <select
        aria-label={label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className={fieldClass}
      >
        {options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

/** The General page of the settings screen. */
export function GeneralDetail() {
  const settings = useSettingsStore((s) => s.settings);
  const error = useSettingsStore((s) => s.error);

  return (
    <section aria-label="General settings" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">General</h2>
      <Picker
        label="Theme"
        value={settings.theme}
        options={THEMES}
        onChange={(mode) => void dispatch(intents.setTheme(mode as 'follow_host' | 'light' | 'dark'))}
      />
      <Picker
        label="Clock"
        value={settings.twenty_four_clock ? '24' : '12'}
        options={[
          { id: '24', label: '24-hour' },
          { id: '12', label: '12-hour' },
        ]}
        onChange={(id) => void dispatch(intents.setClock(id === '24'))}
      />
      {error === 'persist_failed' && (
        <p role="alert" className="text-xs text-[var(--warning)]">
          {PERSIST_SENTENCE}
        </p>
      )}
    </section>
  );
}
