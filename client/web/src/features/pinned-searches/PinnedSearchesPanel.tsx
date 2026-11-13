// SPDX-License-Identifier: GPL-3.0-only
// The pinned searches panel: a popover off the title bar's pin button listing
// the saved queries with their run, rename, and delete verbs. Running one
// opens the search island and dispatches the query alone, exactly as a typed
// search does: the scope is session state and never rides a saved entry.
// Escape and a pointer down outside close it (the pin trigger excepted, so
// its own toggle is not doubled), and the focus moves into the dialog on open
// and back to the trigger on close.

import { useEffect, useState } from 'react';

import { PinIcon } from '@/components/icons';
import { openSearchIsland } from '@/components/layout/searchIsland';
import { useDialogFocus } from '@/components/ui/useDialogFocus';
import { dispatch, intents } from '@/services/ipc';

import { usePinnedSearchesStore, type PinnedSearch } from './pinnedSearchesStore';

const ROW_BUTTON_CLASS =
  'min-w-0 flex-1 rounded-[var(--radius-sm)] px-2 py-1.5 text-left transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

const SMALL_CLASS =
  'shrink-0 rounded px-1.5 py-0.5 text-[11px] text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40';

/** The store gate: the panel stands while the gate is open. */
export function PinnedSearchesPanel() {
  const open = usePinnedSearchesStore((s) => s.panelOpen);
  if (!open) {
    return null;
  }
  return <PinnedSearchesPanelBody />;
}

/** The popover body, mounted only while the gate holds it open. */
function PinnedSearchesPanelBody() {
  const searches = usePinnedSearchesStore((s) => s.searches);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftName, setDraftName] = useState('');
  const dialogRef = useDialogFocus<HTMLDivElement>();

  useEffect(() => {
    function onPointer(event: MouseEvent): void {
      const target = event.target as HTMLElement;
      if (dialogRef.current?.contains(target)) {
        return;
      }
      // The pin trigger toggles the panel itself; closing here too would
      // net its click a second toggle.
      if (target.closest('[data-pinned-searches-bell]') !== null) {
        return;
      }
      usePinnedSearchesStore.getState().closePanel();
    }
    document.addEventListener('mousedown', onPointer);
    return () => document.removeEventListener('mousedown', onPointer);
  }, [dialogRef]);

  const close = (): void => usePinnedSearchesStore.getState().closePanel();

  /** Runs the saved query at the session's standing scope, as typing does. */
  const run = (search: PinnedSearch): void => {
    openSearchIsland();
    void dispatch(intents.search(search.query));
    close();
  };

  const startRename = (search: PinnedSearch): void => {
    setEditingId(search.id);
    setDraftName(search.name);
  };

  const saveRename = (): void => {
    if (editingId === null || draftName.trim() === '') {
      return;
    }
    usePinnedSearchesStore.getState().rename(editingId, draftName);
    setEditingId(null);
  };

  return (
    <div
      ref={dialogRef}
      role="dialog"
      aria-label="Saved searches"
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.preventDefault();
          close();
        }
      }}
      className="flex w-[340px] max-w-[calc(100vw-24px)] flex-col rounded-[var(--radius-sm)] border border-border-subtle bg-surface-floating shadow-[var(--shadow-lg)] outline-none"
    >
      <div className="border-b border-border-subtle px-3 py-2">
        <h2 className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
          <PinIcon size={14} />
          Saved searches
        </h2>
        <p className="mt-0.5 text-[11px] text-muted-text">Only the query is saved, never the scope.</p>
      </div>

      <div className="max-h-[420px] overflow-y-auto py-1">
        {searches.length === 0 ? (
          <p className="px-3 py-6 text-center text-xs text-muted-text">No saved searches yet.</p>
        ) : (
          <ul className="flex flex-col gap-0.5">
            {searches.map((search) => (
              <li key={search.id} className="flex items-start justify-between gap-1 px-1">
                {editingId === search.id ? (
                  <form
                    className="flex min-w-0 flex-1 items-center gap-1 px-2 py-1.5"
                    onSubmit={(event) => {
                      event.preventDefault();
                      saveRename();
                    }}
                  >
                    <input
                      type="text"
                      aria-label="Name"
                      value={draftName}
                      onChange={(event) => setDraftName(event.target.value)}
                      autoFocus
                      className="h-7 min-w-0 flex-1 rounded-[var(--radius-xs)] border border-border bg-background px-2 text-xs text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    />
                    <button type="submit" disabled={draftName.trim() === ''} className={SMALL_CLASS}>
                      Save
                    </button>
                    <button type="button" onClick={() => setEditingId(null)} className={SMALL_CLASS}>
                      Cancel
                    </button>
                  </form>
                ) : (
                  <>
                    <button
                      type="button"
                      onClick={() => run(search)}
                      aria-label={'Run ' + search.name}
                      className={ROW_BUTTON_CLASS}
                    >
                      <span className="block truncate text-xs font-medium text-foreground">
                        {search.name}
                      </span>
                      <span className="mt-0.5 block truncate text-[10px] text-muted-text">
                        {search.query}
                      </span>
                    </button>
                    <button
                      type="button"
                      onClick={() => startRename(search)}
                      aria-label={'Rename ' + search.name}
                      className={SMALL_CLASS}
                    >
                      Rename
                    </button>
                    <button
                      type="button"
                      onClick={() => usePinnedSearchesStore.getState().remove(search.id)}
                      aria-label={'Delete ' + search.name}
                      className={SMALL_CLASS}
                    >
                      Delete
                    </button>
                  </>
                )}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
