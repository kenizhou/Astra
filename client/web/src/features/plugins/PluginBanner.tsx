// SPDX-License-Identifier: GPL-3.0-only
// The plugin banner: the reading pane's slot for a plugin decode's security
// banner. The level picks the tone (a neutral statement, a warning, a broken
// signature's distrust), and the text is the plugin's own words, verbatim.

import type { SecurityBannerView } from '@/services/plugins';

/** The tone classes a banner level reads in. */
const tones: Record<SecurityBannerView['level'], string> = {
  info: 'border-[var(--hairline)] bg-muted/40 text-muted-foreground',
  warning: 'border-warning/40 bg-warning/10 text-warning',
  error: 'border-destructive/40 bg-destructive/10 text-destructive',
};

/** The level's spoken word, for assistive technology beside the colour. */
const words: Record<SecurityBannerView['level'], string> = {
  info: 'Note',
  warning: 'Warning',
  error: 'Error',
};

export function PluginBanner({ banner }: { banner: SecurityBannerView }) {
  return (
    <div
      role="status"
      className={`mx-4 mt-2 rounded-[var(--radius-xs)] border px-3 py-2 text-[12px] ${tones[banner.level]}`}
    >
      <span className="font-medium">{words[banner.level]}: </span>
      {banner.text}
    </div>
  );
}
