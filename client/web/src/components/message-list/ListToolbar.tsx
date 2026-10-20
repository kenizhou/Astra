// SPDX-License-Identifier: GPL-3.0-only
// The list pane's toolbar: the open folder's name on the left and, on the
// right, the filter's segmented control beside the flat/threaded one.
// The view mode crosses back as one set_view_mode intent per click; the
// filter stays on the client, its state held by the list-filter store,
// because the core owes no filtered view of its own. A live search
// replaces the folder's own toolbar: the result count and the depth note
// stand where the folder name stood (a zero horizon claims no note), the
// filter control hides (its tabs derive from the folder the search is not
// showing), and the exit affordance clears the search back to the folder
// view. The outbox mode keeps only the title: the queue has no filter and
// no grouping mode.

import { dispatch, intents } from '@/services/ipc';
import type { MessageRowView, SearchState, ViewMode } from '@/services/surfaces';
import type { ListFilter } from './listFilter';

const MODES: ReadonlyArray<{ id: ViewMode; label: string }> = [
  { id: 'flat', label: 'Flat' },
  { id: 'threaded', label: 'Threaded' },
];

/** The bulk actions the toolbar owes while a selection stands. */
export interface BulkBarProps {
  /** The selected rows: the batch every action loops. */
  rows: MessageRowView[];
  /** Selects every loaded row. */
  onSelectAll: () => void;
  /** Clears the selection. */
  onClear: () => void;
}

/** The bulk bar: the count, Select All, Clear, and the batch actions looping
 *  the per-row intents exactly as the row menu does, so a selection never
 *  depends on the context menu. */
export function BulkBar({ rows, onSelectAll, onClear }: BulkBarProps) {
  const anyUnread = rows.some((row) => row.unread);
  const allFlagged = rows.every((row) => row.flagged);
  const button =
    'whitespace-nowrap rounded-[var(--radius-xs)] px-2 py-0.5 text-xs text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';
  const action = (label: string, act: () => void): React.ReactNode => (
    <button key={label} type="button" className={button} onClick={act}>
      {label}
    </button>
  );
  return (
    <div
      role="group"
      aria-label="Selection actions"
      className="flex h-[var(--pane-header-h)] shrink-0 items-center gap-2 overflow-hidden border-b border-border-subtle bg-muted/40 px-3"
    >
      <span role="status" className="shrink-0 text-[13px] font-semibold text-foreground">
        {rows.length === 1 ? '1 selected' : `${rows.length} selected`}
      </span>
      <div className="flex min-w-0 flex-1 items-center gap-1">
        {action(anyUnread ? 'Mark as Read' : 'Mark as Unread', () =>
          forEach(rows, (row) => void dispatch(intents.markRead(row.key, row.unread))),
        )}
        {action(allFlagged ? 'Unflag' : 'Flag', () =>
          forEach(rows, (row) => void dispatch(intents.setFlagged(row.key, !allFlagged))),
        )}
        {action('Archive', () =>
          forEach(rows, (row) => void dispatch(intents.archiveMessage(row.key))),
        )}
        {action('Delete', () => forEach(rows, (row) => void dispatch(intents.moveToTrash(row.key))))}
      </div>
      <button type="button" className={button} onClick={onSelectAll}>
        Select All
      </button>
      <button type="button" className={button} onClick={onClear}>
        Clear
      </button>
    </div>
  );
}

/** Fires one intent per selected row, fire-and-forget: the row menu's own
 *  loop, shared. */
function forEach(rows: MessageRowView[], act: (row: MessageRowView) => void): void {
  for (const row of rows) {
    act(row);
  }
}

/** The tabs the filter control offers, the badge's count beside each. */
const FILTERS: ReadonlyArray<{ id: ListFilter; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'unread', label: 'Unread' },
  { id: 'flagged', label: 'Flagged' },
  { id: 'attachments', label: 'Attachments' },
];

interface ListToolbarProps {
  /** The folder whose mail the list shows; its display name, or none. */
  folderName: string | null;
  /** The grouping mode the snapshot last reported. */
  viewMode: ViewMode;
  /** The tab the filter is showing. */
  filter: ListFilter;
  /** The counts the badges draw, over the unfiltered rows. */
  filterCounts: Record<ListFilter, number>;
  /** Records the tab the user chose. */
  onFilter: (filter: ListFilter) => void;
  /** The live search state; its results header replaces the folder's own. */
  search: SearchState | null;
  /** Whether the queue view leads: the title alone, no controls. */
  outbox?: boolean;
  /** The bulk bar's rows and controls, while a selection stands. */
  bulk?: BulkBarProps | null;
}

export function ListToolbar({ folderName, viewMode, filter, filterCounts, onFilter, search, outbox = false, bulk = null }: ListToolbarProps) {
  const switchMode = (mode: ViewMode) => {
    void dispatch(intents.setViewMode(mode));
  };
  const exitSearch = () => {
    void dispatch(intents.search(null));
  };

  if (outbox) {
    return (
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between gap-2 overflow-hidden border-b border-border-subtle px-3">
        <span className="min-w-0 truncate text-[13px] font-semibold text-foreground">Outbox</span>
      </div>
    );
  }

  return (
    <div className="shrink-0">
      {bulk !== null && <BulkBar {...bulk} />}
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between gap-2 overflow-hidden border-b border-border-subtle px-3">
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
        <div className="flex min-w-0 items-center gap-2">
          {search === null && (
            <div
              role="group"
              aria-label="Filter"
              className="flex shrink-0 items-center gap-0.5 rounded-md bg-muted p-0.5"
            >
              {FILTERS.map(({ id, label }) => (
                <button
                  key={id}
                  type="button"
                  aria-pressed={filter === id}
                  onClick={() => onFilter(id)}
                  className={`whitespace-nowrap rounded-[var(--radius-xs)] px-2 py-0.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                    filter === id
                      ? 'bg-background font-medium text-foreground shadow-[var(--shadow-sm)]'
                      : 'text-muted-text hover:text-foreground'
                  }`}
                >
                  {label} {filterCounts[id]}
                </button>
              ))}
            </div>
          )}
          <div
            role="group"
            aria-label="View mode"
            className="flex shrink-0 items-center gap-0.5 rounded-md bg-muted p-0.5"
          >
            {MODES.map(({ id, label }) => (
              <button
                key={id}
                type="button"
                aria-pressed={viewMode === id}
                onClick={() => switchMode(id)}
                className={`whitespace-nowrap rounded-[var(--radius-xs)] px-2 py-0.5 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
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
              className="whitespace-nowrap rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Exit search
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
