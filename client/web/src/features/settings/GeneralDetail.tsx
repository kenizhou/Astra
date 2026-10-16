// SPDX-License-Identifier: GPL-3.0-only
// The General detail: the theme picker (follow host, Light, Dark), the
// clock picker (24 or 12 hour), and the device-local appearance choices
// (accent skin, texture, contrast, font size, density). The pickers
// dispatch their intents and reflect the persisted block, never local
// optimism, and the persist failure's sentence renders under the controls
// it belongs to. The appearance choices are different: the persisted
// settings block has no fields for them, so they write localStorage and
// the document-root attributes through the appearance store and stay on
// this device.

import { dispatch, intents } from '@/services/ipc';
import { SKINS, useAppearanceStore, type ContrastMode, type DensityMode, type FontSizeStep, type ReadingPosition, type TextureMode } from '@/services/appearance';

import { useSettingsStore } from './settingsStore';

/** The theme choices, in the taxonomy's own order. */
const THEMES: ReadonlyArray<{ id: 'follow_host' | 'light' | 'dark'; label: string }> = [
  { id: 'follow_host', label: 'Follow host' },
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
];

/** The contrast choices. */
const CONTRASTS: ReadonlyArray<{ id: ContrastMode; label: string }> = [
  { id: 'default', label: 'Default' },
  { id: 'high', label: 'High' },
];

/** The font-size steps: 13, 14, and 16px on the body. */
const FONT_SIZES: ReadonlyArray<{ id: FontSizeStep; label: string }> = [
  { id: 'small', label: 'Small' },
  { id: 'default', label: 'Default' },
  { id: 'large', label: 'Large' },
];

/** The density steps: comfortable is the default; compact tightens the axis. */
const DENSITIES: ReadonlyArray<{ id: DensityMode; label: string }> = [
  { id: 'comfortable', label: 'Comfortable' },
  { id: 'compact', label: 'Compact' },
];

/** The reading pane's dock: right of the list, beneath it, or hidden. */
const READING_POSITIONS: ReadonlyArray<{ id: ReadingPosition; label: string }> = [
  { id: 'right', label: 'Right' },
  { id: 'bottom', label: 'Bottom' },
  { id: 'off', label: 'Off' },
];

/** The texture steps: warm is the default; calm its quiet sibling; classic
 *  the pre-texture cool slate. */
const TEXTURES: ReadonlyArray<{ id: TextureMode; label: string }> = [
  { id: 'warm', label: 'Warm' },
  { id: 'calm', label: 'Calm' },
  { id: 'classic', label: 'Classic' },
];

/** The sentence a refused persist draws. */
export const PERSIST_SENTENCE = 'The change could not be saved. Try again.';

/** The select chrome every picker in the detail shares. */
const fieldClass =
  'h-7 w-full rounded-full border border-input bg-background px-3 text-xs text-foreground outline-none transition-colors focus:border-primary';

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

/** A segmented row of choices, one pressed at a time. */
function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: ReadonlyArray<{ id: T; label: string }>;
  onChange: (id: T) => void;
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs text-muted-foreground">{label}</span>
      <div role="group" aria-label={label} className="flex gap-1">
        {options.map((option) => (
          <button
            key={option.id}
            type="button"
            aria-pressed={value === option.id}
            onClick={() => onChange(option.id)}
            className={`rounded-[var(--radius-xs)] px-3 py-1.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
              value === option.id
                ? 'bg-primary-muted font-medium text-primary'
                : 'border border-border text-muted-foreground hover:bg-hover hover:text-foreground'
            }`}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}

/** The accent skin swatches; the pressed one wears the ring. */
function SkinSwatches() {
  const skin = useAppearanceStore((s) => s.skin);
  const setSkin = useAppearanceStore((s) => s.setSkin);

  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs text-muted-foreground">Accent</span>
      <div role="group" aria-label="Accent" className="flex flex-wrap gap-2">
        {SKINS.map((entry) => {
          const selected = skin === entry.id;
          return (
            <button
              key={entry.id}
              type="button"
              aria-label={`${entry.label} accent`}
              aria-pressed={selected}
              title={entry.label}
              onClick={() => setSkin(entry.id)}
              className={`flex flex-col items-center gap-1 rounded-lg p-1.5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                selected ? 'bg-primary-muted ring-2 ring-primary' : 'hover:bg-hover'
              }`}
            >
              <span
                className="h-8 w-8 rounded-full shadow-[var(--shadow-sm)]"
                style={{ background: entry.swatch }}
              />
              <span
                className={`text-[11px] font-medium ${selected ? 'text-primary' : 'text-muted-foreground'}`}
              >
                {entry.label}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** The General page of the settings screen. */
export function GeneralDetail() {
  const settings = useSettingsStore((s) => s.settings);
  const error = useSettingsStore((s) => s.error);
  const contrast = useAppearanceStore((s) => s.contrast);
  const setContrast = useAppearanceStore((s) => s.setContrast);
  const fontSize = useAppearanceStore((s) => s.fontSize);
  const setFontSize = useAppearanceStore((s) => s.setFontSize);
  const density = useAppearanceStore((s) => s.density);
  const setDensity = useAppearanceStore((s) => s.setDensity);
  const texture = useAppearanceStore((s) => s.texture);
  const readingPosition = useAppearanceStore((s) => s.readingPosition);
  const setReadingPosition = useAppearanceStore((s) => s.setReadingPosition);
  const setTexture = useAppearanceStore((s) => s.setTexture);

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
      <h3 className="pt-2 text-xs font-semibold text-muted-foreground">
        Appearance (this device only)
      </h3>
      <SkinSwatches />
      <Segmented label="Texture" value={texture} options={TEXTURES} onChange={setTexture} />
      <Segmented label="Contrast" value={contrast} options={CONTRASTS} onChange={setContrast} />
      <Segmented label="Font size" value={fontSize} options={FONT_SIZES} onChange={setFontSize} />
      <Segmented label="Density" value={density} options={DENSITIES} onChange={setDensity} />
      <Segmented label="Reading pane" value={readingPosition} options={READING_POSITIONS} onChange={setReadingPosition} />
    </section>
  );
}
