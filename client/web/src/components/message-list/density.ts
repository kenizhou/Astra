// SPDX-License-Identifier: GPL-3.0-only
// The message list's density table: the three row heights the View menu and
// the status bar's cycle offer, each spelling the row's minimum height and
// content padding the way KY's row chrome does. The pixel figure doubles as
// the virtualiser's row estimate, so the window and the paint never disagree
// about how tall a row stands.

/** The densities the list offers, in the cycle's order. */
export type Density = 'compact' | 'normal' | 'comfortable';

export const DENSITIES: ReadonlyArray<{ id: Density; label: string }> = [
  { id: 'compact', label: 'Compact' },
  { id: 'normal', label: 'Normal' },
  { id: 'comfortable', label: 'Comfortable' },
];

/** The row height in pixels per density; the virtualiser's estimate. */
export const DENSITY_ROW_PX: Record<Density, number> = {
  compact: 44,
  normal: 56,
  comfortable: 72,
};

/** The row chrome per density: the minimum height and the content padding. */
export const DENSITY_ROW_CLASS: Record<Density, string> = {
  compact: 'min-h-[44px] py-1',
  normal: 'min-h-[56px] py-2',
  comfortable: 'min-h-[72px] py-3.5',
};

/** The next density the status bar's cycle steps to, wrapping around. */
export function nextDensity(density: Density): Density {
  if (density === 'compact') {
    return 'normal';
  }
  if (density === 'normal') {
    return 'comfortable';
  }
  return 'compact';
}
