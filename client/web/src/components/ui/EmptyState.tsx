// SPDX-License-Identifier: GPL-3.0-only
// The state family (UI1 D3): one EmptyState for every bare empty state, and
// the SkeletonRows that follow the density axis while a surface loads.
// Per-surface content lives in the content map beside the family
// (stateContent.ts), the single source of truth.

import type { ReactNode } from 'react';

/** The empty state: glyph tile, message, optional primary action. */
export function EmptyState({
  glyph,
  message,
  action,
}: {
  glyph: ReactNode;
  message: string;
  action?: { label: string; onClick: () => void };
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 p-4">
      <span className="flex h-12 w-12 items-center justify-center rounded-full bg-primary-muted text-primary">
        {glyph}
      </span>
      <p className="text-sm text-muted-text">{message}</p>
      {action !== undefined && (
        <button
          type="button"
          onClick={action.onClick}
          className="mt-1 min-h-[var(--hit)] rounded-[var(--radius)] bg-primary px-3 text-xs font-medium text-primary-foreground transition-colors hover:bg-[var(--series-accent-hover)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}

/** The loading state: three bars per pane, row-height matched to the axis. */
export function SkeletonRows() {
  return (
    <div aria-hidden="true" className="flex flex-1 flex-col gap-2 p-3">
      {[0, 1, 2].map((index) => (
        <span
          key={index}
          data-skeleton-bar
          className="animate-pulse rounded-[6px] bg-[var(--surface)]"
          style={{ minHeight: 'var(--row-h)' }}
        />
      ))}
    </div>
  );
}
