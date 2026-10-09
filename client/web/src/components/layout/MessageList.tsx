// SPDX-License-Identifier: GPL-3.0-only
// The message list pane: the toolbar over the folder's rows, virtualised
// with @tanstack/react-virtual the way KY windows its list (an absolutely
// positioned row per virtual item, measured as it mounts, a fixed estimate
// before the first measure). The rows come straight from the snapshot the
// pulls keep fresh; a single click selects and opens (selection is open,
// the highlight following the open message), and ShowMore grows the window
// the core owes whenever the rows fill it.

import { useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';

import { ListToolbar } from '@/components/message-list/ListToolbar';
import { MessageRow } from '@/components/message-list/MessageRow';
import { sameRowKey } from '@/components/message-list/rowIdentity';
import { dispatch, intents } from '@/services/ipc';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import type { RowKey } from '@/services/surfaces';

/** The selected folder's display name, straight off the folder tree. */
function folderDisplayName(
  accounts: ReturnType<typeof useFolderStore.getState>['accounts'],
  selected: ReturnType<typeof useFolderStore.getState>['selected'],
): string | null {
  if (selected === null) {
    return null;
  }
  const account = accounts.find((a) => a.account_id === selected.account);
  return account?.folders.find((f) => f.key === selected.key)?.name ?? null;
}

export function MessageList() {
  const rows = useListStore((s) => s.rows);
  const viewMode = useListStore((s) => s.viewMode);
  const visibleLimit = useListStore((s) => s.visibleLimit);
  const selectedKey = useListStore((s) => s.selectedKey);
  const select = useListStore((s) => s.select);
  const openSnapshot = useReadingStore((s) => s.snapshot);
  const accounts = useFolderStore((s) => s.accounts);
  const folder = useFolderStore((s) => s.selected);

  // The highlight follows the open message first: the reading snapshot is
  // the record of what is open, and the local click selection covers the
  // gap before the reading pull lands. Known gap for the thread slice: in
  // the threaded view this message key names no thread row, so it outranks
  // the clicked thread row's local selection and nothing highlights until
  // the open key maps to the thread it belongs to.
  const openKey: RowKey | null =
    openSnapshot === null ? null : { message: openSnapshot.message };
  const highlight = openKey ?? selectedKey;

  const openRow = (key: RowKey): void => {
    select(key);
    // A message row opens the reading pane; a thread row names no single
    // message, so it selects alone until the thread slice lands.
    if ('message' in key) {
      void dispatch(intents.openMessage(key.message));
    }
  };

  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 56,
    overscan: 12,
  });
  const now = new Date();

  const windowIsFull = visibleLimit > 0 && rows.length === visibleLimit;
  const showRows = folder !== null && rows.length > 0;

  return (
    <section
      aria-label="Message list"
      className="flex min-w-0 flex-1 flex-col overflow-hidden bg-background"
    >
      <ListToolbar folderName={folderDisplayName(accounts, folder)} viewMode={viewMode} />
      {folder === null ? (
        <p className="p-4 text-sm text-muted-foreground">Select a folder to read its mail.</p>
      ) : !showRows ? (
        <p className="p-4 text-sm text-muted-foreground">No messages in this folder.</p>
      ) : (
        <div
          ref={scrollRef}
          role="listbox"
          aria-label="Messages"
          className="min-h-0 flex-1 overflow-y-auto"
        >
          <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
            {virtualizer.getVirtualItems().map((item) => {
              const row = rows[item.index];
              if (row === undefined) {
                return null;
              }
              return (
                <div
                  key={item.key}
                  data-index={item.index}
                  ref={virtualizer.measureElement}
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    width: '100%',
                    transform: `translateY(${item.start}px)`,
                  }}
                >
                  <MessageRow
                    row={row}
                    selected={sameRowKey(row.key, highlight)}
                    now={now}
                    onSelect={openRow}
                  />
                </div>
              );
            })}
          </div>
          {windowIsFull && (
            <button
              type="button"
              onClick={() => void dispatch(intents.showMore())}
              className="w-full border-t border-border px-3 py-2 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
            >
              Show more
            </button>
          )}
        </div>
      )}
    </section>
  );
}
