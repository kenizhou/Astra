// SPDX-License-Identifier: GPL-3.0-only
// The calendar's own markers: the repeat glyph a recurring row draws, the
// hold marking an unanswered invitation draws, and the colour swatch a
// calendar's own colour paints. Shared by every view so a row means the same
// thing wherever it renders.

import type { OccurrenceRow } from '@/services/ipc';

/** The stroke-drawn repeat glyph. */
export function RepeatGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      width="10"
      height="10"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className="shrink-0"
    >
      <path d="m17 2 4 4-4 4" />
      <path d="M3 11v-1a4 4 0 0 1 4-4h14" />
      <path d="m7 22-4-4 4-4" />
      <path d="M21 13v1a4 4 0 0 1-4 4H3" />
    </svg>
  );
}

/** The calendar's colour as a dot. */
export function CalendarDot({ color }: { color: string | undefined }) {
  return (
    <span
      aria-hidden="true"
      className="inline-block h-2 w-2 shrink-0 rounded-full bg-primary"
      style={color !== undefined ? { backgroundColor: color } : undefined}
    />
  );
}

/** Whether a row draws as an unanswered hold. */
export function isHold(row: OccurrenceRow): boolean {
  return row.participation === 'needs_action';
}

/** The block's hold class: dashed and half-present, the unanswered idiom. */
export const holdClass = (hold: boolean): string =>
  hold ? 'border-dashed bg-transparent opacity-70' : 'border-solid';
