// SPDX-License-Identifier: GPL-3.0-only
// The list pane's toolbar: the open folder's name on the left and, on the
// right, the split inbox's segmented control beside the flat/threaded one.
// The view mode crosses back as one set_view_mode intent per click; the
// split filter stays on the client, its state held by the split store,
// because the core owes no side view of its own.

import { dispatch, intents } from '@/services/ipc';
import type { ViewMode } from '@/services/surfaces';
import type { SplitFilter } from '@/stores/splitStore';

const MODES: ReadonlyArray<{ id: ViewMode; label: string }> = [
  { id: 'flat', label: 'Flat' },
  { id: 'threaded', label: 'Threaded' },
];

/** The sides the split control offers, the badge's count beside each. */
const SIDES: ReadonlyArray<{ id: SplitFilter; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'actioned', label: 'Actioned' },
  { id: 'fyi', label: 'FYI' },
];

interface ListToolbarProps {
  /** The folder whose mail the list shows; its display name, or none. */
  folderName: string | null;
  /** The grouping mode the snapshot last reported. */
  viewMode: ViewMode;
  /** The side the split filter is showing. */
  splitFilter: SplitFilter;
  /** The counts the badges draw: the whole list and each half. */
  splitCounts: { all: number; actioned: number; fyi: number };
  /** Records the side the user chose. */
  onSplitFilter: (filter: SplitFilter) => void;
}

export function ListToolbar({ folderName, viewMode, splitFilter, splitCounts, onSplitFilter }: ListToolbarProps) {
  const switchMode = (mode: ViewMode) => {
    void dispatch(intents.setViewMode(mode));
  };

  return (
    <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between gap-2 border-b border-border bg-surface px-3">
      <span className="min-w-0 truncate text-[13px] font-semibold text-foreground">
        {folderName ?? 'Mail'}
      </span>
      <div className="flex items-center gap-3">
        <div role="group" aria-label="Split" className="flex items-center gap-1">
          {SIDES.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              aria-pressed={splitFilter === id}
              onClick={() => onSplitFilter(id)}
              className={`rounded-[var(--radius-xs)] px-2 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                splitFilter === id
                  ? 'bg-selected font-medium text-foreground'
                  : 'text-muted-foreground hover:bg-hover hover:text-foreground'
              }`}
            >
              {label} {splitCounts[id]}
            </button>
          ))}
        </div>
        <div role="group" aria-label="View mode" className="flex items-center gap-1">
          {MODES.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              aria-pressed={viewMode === id}
              onClick={() => switchMode(id)}
              className={`rounded-[var(--radius-xs)] px-2 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                viewMode === id
                  ? 'bg-selected font-medium text-foreground'
                  : 'text-muted-foreground hover:bg-hover hover:text-foreground'
              }`}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
