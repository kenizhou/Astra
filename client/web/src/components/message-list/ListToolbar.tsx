// SPDX-License-Identifier: GPL-3.0-only
// The list pane's toolbar: the open folder's name on the left and the
// flat/threaded segmented control on the right. The control crosses back
// as one set_view_mode intent per click; the state itself lives in the
// core and returns with the next snapshot.

import { dispatch, intents } from '@/services/ipc';
import type { ViewMode } from '@/services/surfaces';

const MODES: ReadonlyArray<{ id: ViewMode; label: string }> = [
  { id: 'flat', label: 'Flat' },
  { id: 'threaded', label: 'Threaded' },
];

interface ListToolbarProps {
  /** The folder whose mail the list shows; its display name, or none. */
  folderName: string | null;
  /** The grouping mode the snapshot last reported. */
  viewMode: ViewMode;
}

export function ListToolbar({ folderName, viewMode }: ListToolbarProps) {
  const switchMode = (mode: ViewMode) => {
    void dispatch(intents.setViewMode(mode));
  };

  return (
    <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between gap-2 border-b border-border bg-surface px-3">
      <span className="min-w-0 truncate text-[13px] font-semibold text-foreground">
        {folderName ?? 'Mail'}
      </span>
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
  );
}
