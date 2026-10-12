// SPDX-License-Identifier: GPL-3.0-only
// The tool rail's button shape, shared by the rail's own buttons and the
// outbox, agent, and AI documents toggles that ride it.

export const RAIL_BUTTON_CLASS =
  'relative grid h-12 w-12 place-items-center rounded-lg transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

/** The rail button's idle and active colour pairings. */
export function railButtonColours(active: boolean): string {
  return active
    ? 'bg-primary-muted text-primary'
    : 'text-muted-text hover:bg-hover hover:text-foreground';
}
