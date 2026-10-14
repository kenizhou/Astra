// SPDX-License-Identifier: GPL-3.0-only
// The message list pane: the toolbar over the folder's rows, virtualised
// with @tanstack/react-virtual the way KY windows its list (an absolutely
// positioned row per virtual item, measured as it mounts, a fixed estimate
// before the first measure). The rows come straight from the snapshot the
// pulls keep fresh, grouped under a date header per sent-at day; the split
// filter narrows what renders, never the window itself, so a full window
// keeps offering to grow. A single click selects and opens (selection is
// open, the highlight following the open message), and ShowMore grows the
// window the core owes whenever the rows fill it. A live search state
// replaces the folder rows with the result rows through the same window and
// the same open flow, keyed by message, flat with no date headers. The
// folder tree's Outbox row swaps the whole body for the account's queue
// view. Row heights follow the density the list store holds.

import { useEffect, useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';

import { ListToolbar } from '@/components/message-list/ListToolbar';
import { MessageRow } from '@/components/message-list/MessageRow';
import { EmptyState } from '@/components/ui/EmptyState';
import { STATE_CONTENT } from '@/components/ui/stateContent';
import { MailIcon } from '@/components/icons';
import {
  HEADER_ROW_PX,
  flatItems,
  withDateHeaders,
  type ListItem,
} from '@/components/message-list/dateGroups';
import { DENSITY_ROW_PX } from '@/components/message-list/density';
import { sameRowKey } from '@/components/message-list/rowIdentity';
import { OutboxQueue } from '@/features/outbox/OutboxQueue';
import { dispatch, intents } from '@/services/ipc';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import { splitCounts, useSplitStore, visibleRows } from '@/stores/splitStore';
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

interface RowWindowProps {
  /** The items the mode is showing, headers interleaved or flat. */
  items: ListItem[];
  /** The key the highlight follows: the open message, or the local click. */
  highlight: RowKey | null;
  /** The clock the stamps are formatted against. */
  now: Date;
  /** Records the local selection when a row is clicked. */
  openRow: (key: RowKey) => void;
  /** Whether the grow affordance shows; only the folder window can fill. */
  showMore: boolean;
}

/** The virtualised row window both list modes draw their rows through. */
function RowWindow({ items, highlight, now, openRow, showMore }: RowWindowProps) {
  const density = useListStore((s) => s.density);
  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) =>
      items[index]?.kind === 'header' ? HEADER_ROW_PX : DENSITY_ROW_PX[density],
    overscan: 12,
  });

  return (
    <div
      ref={scrollRef}
      role="listbox"
      aria-label="Messages"
      className="min-h-0 flex-1 overflow-y-auto"
    >
      <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
        {virtualizer.getVirtualItems().map((item) => {
          const entry = items[item.index];
          if (entry === undefined) {
            return null;
          }
          return (
            <div
              key={`${String(item.key)}:${density}`}
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
              {entry.kind === 'header' ? (
                <div
                  role="presentation"
                  className="px-3 pb-1 pt-3 text-[11px] font-medium uppercase tracking-wide text-muted-text"
                >
                  {entry.label}
                </div>
              ) : (
                <MessageRow
                  row={entry.row}
                  selected={sameRowKey(entry.row.key, highlight)}
                  now={now}
                  onSelect={openRow}
                />
              )}
            </div>
          );
        })}
      </div>
      {showMore && (
        <button
          type="button"
          onClick={() => void dispatch(intents.showMore())}
          className="w-full px-3 py-2 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
        >
          Show more
        </button>
      )}
    </div>
  );
}

export function MessageList() {
  const rows = useListStore((s) => s.rows);
  const viewMode = useListStore((s) => s.viewMode);
  const visibleLimit = useListStore((s) => s.visibleLimit);
  const selectedKey = useListStore((s) => s.selectedKey);
  const select = useListStore((s) => s.select);
  const search = useListStore((s) => s.search);
  const openSnapshot = useReadingStore((s) => s.snapshot);
  const accounts = useFolderStore((s) => s.accounts);
  const folder = useFolderStore((s) => s.selected);
  const outboxAccount = useFolderStore((s) => s.outboxAccount);
  const splitFilter = useSplitStore((s) => s.filter);
  const entries = useSplitStore((s) => s.entries);

  const outboxMode = outboxAccount !== null;
  const searching = search !== null;
  const shownRows = visibleRows(rows, splitFilter, entries);
  const counts = splitCounts(rows, entries);

  // The highlight follows the open message first: the reading snapshot is
  // the record of what is open, and the local click selection covers the
  // gap before the reading pull lands. Known gap for the thread slice: in
  // the threaded view this message key names no thread row, so it outranks
  // the clicked thread row's local selection and nothing highlights until
  // the open key maps to the thread it belongs to.
  const openKey: RowKey | null =
    openSnapshot === null ? null : { message: openSnapshot.message };
  const highlight = openKey ?? selectedKey;

  // A view that hides the selected row takes the selection with it: the
  // highlight must never name a row the pane does not show, whichever row
  // set the live mode is drawing.
  const viewRows = searching ? search.rows : shownRows;
  const selectedHidden =
    selectedKey !== null && !viewRows.some((row) => sameRowKey(row.key, selectedKey));
  useEffect(() => {
    if (selectedHidden) {
      useListStore.getState().select(null);
    }
  }, [selectedHidden]);

  const openRow = (key: RowKey): void => {
    select(key);
    // A message row opens the reading pane; a thread row names no single
    // message, so it selects alone until the thread slice lands.
    if ('message' in key) {
      void dispatch(intents.openMessage(key.message));
    }
  };

  const now = new Date();

  // The window compares against every row the snapshot sent, not the side
  // view: rows behind the load window may belong to the chosen side, so a
  // full window keeps offering to grow whatever the filter shows.
  const windowIsFull = visibleLimit > 0 && rows.length === visibleLimit;
  const showRows = folder !== null && rows.length > 0;

  return (
    <section
      aria-label="Message list"
      className="flex min-w-0 flex-1 flex-col overflow-hidden bg-background"
    >
      <ListToolbar
        folderName={folderDisplayName(accounts, folder)}
        viewMode={viewMode}
        splitFilter={splitFilter}
        splitCounts={counts}
        onSplitFilter={(filter) => useSplitStore.getState().setFilter(filter)}
        search={search}
        outbox={outboxMode}
      />
      {outboxMode ? (
        <OutboxQueue account={outboxAccount} />
      ) : searching ? (
        search.rows.length === 0 ? (
          <EmptyState glyph={<MailIcon size={24} />} message={STATE_CONTENT.searchEmpty!.message} />
        ) : (
          <RowWindow items={flatItems(search.rows)} highlight={highlight} now={now} openRow={openRow} showMore={false} />
        )
      ) : folder === null ? (
        <EmptyState glyph={<MailIcon size={24} />} message={STATE_CONTENT.listNoFolder!.message} />
      ) : !showRows ? (
        <EmptyState glyph={<MailIcon size={24} />} message={STATE_CONTENT.listEmptyFolder!.message} />
      ) : shownRows.length === 0 ? (
        <EmptyState glyph={<MailIcon size={24} />} message={STATE_CONTENT.listEmptySide!.message} />
      ) : (
        <RowWindow items={withDateHeaders(shownRows, now)} highlight={highlight} now={now} openRow={openRow} showMore={windowIsFull} />
      )}
    </section>
  );
}
