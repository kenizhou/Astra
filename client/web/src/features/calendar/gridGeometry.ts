// SPDX-License-Identifier: GPL-3.0-only
// The time grid's geometry: the fixed pixel sizes the hour rows and day
// columns are drawn at, and the pointer-to-grid conversion a drag reads. Pure
// constants and math, shared by the day and work-week grids so both draw and
// drag on one scale.

/** The hour gutter's width, in pixels. */
export const GUTTER_W = 56;

/** One hour row's height, in pixels. */
export const HOUR_H = 44;

/** Pixels per minute on the grid. */
export const PX_PER_MINUTE = HOUR_H / 60;

/** The column width the pointer math falls back to when the grid has no box,
 * which only a hidden or headless grid ever has. */
export const FALLBACK_COL_W = 100;

/** One position a pointer holds on the grid. */
export interface GridPoint {
  /** The day column, 0-based. */
  day: number;
  /** The minutes since the column's midnight. */
  minutes: number;
}

/** The total height of a 24-hour grid, in pixels. */
export const DAY_H = 24 * HOUR_H;

/** Minutes since midnight as the grid's top offset, in pixels. */
export function minuteTop(minutes: number): number {
  // Rounded to hundredths so a minute's pixel offset renders (and asserts)
  // as the same number every time, float arithmetic aside.
  return Math.round(minutes * PX_PER_MINUTE * 100) / 100;
}

/**
 * A pointer position as a grid point, against the columns container's box.
 * `dayCount` shares the box's width; minutes clamp into the day so a pointer
 * over the header or past midnight still names a slot.
 */
export function clientToGrid(
  x: number,
  y: number,
  rect: { top: number; left: number; width: number },
  dayCount: number,
): GridPoint {
  const bodyW = rect.width > GUTTER_W ? rect.width - GUTTER_W : dayCount * FALLBACK_COL_W;
  const day = Math.min(dayCount - 1, Math.max(0, Math.floor((x - rect.left - GUTTER_W) / (bodyW / dayCount))));
  const minutes = Math.min(24 * 60, Math.max(0, (y - rect.top) / PX_PER_MINUTE));
  return { day, minutes };
}
