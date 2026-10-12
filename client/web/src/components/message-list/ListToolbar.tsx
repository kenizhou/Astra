// SPDX-License-Identifier: GPL-3.0-only
// The list pane's toolbar: the open folder's name on the left and, on the
// right, the split inbox's segmented control beside the flat/threaded one.
// The view mode crosses back as one set_view_mode intent per click; the
// split filter stays on the client, its state held by the split store,
// because the core owes no side view of its own. A live search replaces
// the folder's own toolbar: the result count and the depth note stand
// where the folder name stood (a zero horizon claims no note), the split
// control hides (its sides derive
// from the folder the search is not showing), and the exit affordance
// clears the search back to the folder view.

import { dispatch, intents } from '@/services/ipc';
import type { SearchState, ViewMode } from '@/services/surfaces';
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
  /** The live search state; its results header replaces the folder's own. */
  search: SearchState | null;
}

export function ListToolbar({ folderName, viewMode, splitFilter, splitCounts, onSplitFilter, search }: ListToolbarProps) {
  const switchMode = (mode: ViewMode) => {
    void dispatch(intents.setViewMode(mode));
  };
  const exitSearch = () => {
    void dispatch(intents.search(null));
  };

  return (
    <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between gap-2 border-b border-border-subtle bg-surface px-3">
      {search === null ? (
        <span className="min-w-0 truncate text-[13px] font-semibold text-foreground">
          {folderName ?? 'Mail'}
        </span>
      ) : (
        <span className="flex min-w-0 items-baseline gap-2">
          <span className="shrink-0 text-[13px] font-semibold text-foreground">
            {search.rows.length === 1 ? '1 result' : `${search.rows.length} results`}
          </span>
          {search.depth_months > 0 && (
            <span className="min-w-0 truncate text-xs text-muted-foreground">
              {search.depth_months === 1
                ? 'Searched the last month'
                : `Searched the last ${search.depth_months} months`}
            </span>
          )}
        </span>
      )}
      <div className="flex items-center gap-2">
        {search === null && (
          <div
            role="group"
            aria-label="Split"
            className="flex items-center gap-0.5 rounded-md bg-muted p-0.5"
          >
            {SIDES.map(({ id, label }) => (
              <button
                key={id}
                type="button"
                aria-pressed={splitFilter === id}
                onClick={() => onSplitFilter(id)}
                className={`rounded-[var(--radius-xs)] px-2 py-0.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                  splitFilter === id
                    ? 'bg-background font-medium text-foreground shadow-[var(--shadow-sm)]'
                    : 'text-muted-text hover:text-foreground'
                }`}
              >
                {label} {splitCounts[id]}
              </button>
            ))}
          </div>
        )}
        <div
          role="group"
          aria-label="View mode"
          className="flex items-center gap-0.5 rounded-md bg-muted p-0.5"
        >
          {MODES.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              aria-pressed={viewMode === id}
              onClick={() => switchMode(id)}
              className={`rounded-[var(--radius-xs)] px-2 py-0.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                viewMode === id
                  ? 'bg-background font-medium text-foreground shadow-[var(--shadow-sm)]'
                  : 'text-muted-text hover:text-foreground'
              }`}
            >
              {label}
            </button>
          ))}
        </div>
        {search !== null && (
          <button
            type="button"
            onClick={exitSearch}
            className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Exit search
          </button>
        )}
      </div>
    </div>
  );
}
