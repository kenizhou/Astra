// SPDX-License-Identifier: GPL-3.0-only
// The message list pane: the toolbar over the folder's rows, virtualised
// with @tanstack/react-virtual the way KY windows its list (an absolutely
// positioned row per virtual item, measured as it mounts, a fixed estimate
// before the first measure). The rows come straight from the snapshot the
// pulls keep fresh, grouped under a date header per sent-at day; the
// client-side filter narrows what renders, never the window itself, so a
// full window keeps offering to grow. A single click selects and opens
// (selection is open, the highlight following the open message), and
// ShowMore grows the window the core owes whenever the rows fill it. A
// live search state replaces the folder rows with the result rows through
// the same window and the same open flow, keyed by message, flat with no
// date headers. The folder tree's Outbox row swaps the whole body for the
// account's queue view. Row heights follow the density the list store
// holds.

import { useEffect, useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';

import { ListToolbar } from '@/components/message-list/ListToolbar';
import { MessageRow } from '@/components/message-list/MessageRow';
import { ContextMenu } from '@/components/ui/ContextMenu';
import { useContextMenu } from '@/components/ui/useContextMenu';
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
import { filteredRows, filterCounts } from '@/components/message-list/listFilter';
import { allKeys, rangeKeys, resolveTargets, toggleKey } from '@/components/message-list/listSelection';
import { buildDraftMenu, buildRowMenu, type RowAccountContext } from '@/components/message-list/rowMenu';
import { SwipeToast } from '@/components/message-list/SwipeToast';
import { sameRowKey } from '@/components/message-list/rowIdentity';
import { OutboxQueue } from '@/features/outbox/OutboxQueue';
import { dispatch, intents } from '@/services/ipc';
import { useFolderStore } from '@/stores/folderStore';
import { useListFilterStore } from '@/stores/listFilterStore';
import { useListStore } from '@/stores/listStore';
import { useReadingStore } from '@/stores/readingStore';
import type { MessageRowView, RowKey } from '@/services/surfaces';

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
  /** The keys the multi-selection holds. */
  selectedKeys: RowKey[];
  /** Whether the rows wear the Draft chip (the open folder is Drafts). */
  drafts: boolean;
  /** The clock the stamps are formatted against. */
  now: Date;
  /** Handles a row's click, modifiers and all. */
  openRow: (key: RowKey, event: React.MouseEvent) => void;
  /** Opens a row's context menu at the cursor. */
  rowMenu: (key: RowKey, event: React.MouseEvent) => void;
  /** Whether the grow affordance shows; only the folder window can fill. */
  showMore: boolean;
}

/** The virtualised row window both list modes draw their rows through. */
function RowWindow({ items, highlight, selectedKeys, drafts, now, openRow, rowMenu, showMore }: RowWindowProps) {
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
      aria-multiselectable="true"
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
                  multiSelected={selectedKeys.some((key) => sameRowKey(key, entry.row.key))}
                  draft={drafts}
                  now={now}
                  onSelect={openRow}
                  onContextMenu={rowMenu}
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
  const selectedKeys = useListStore((s) => s.selectedKeys);
  const select = useListStore((s) => s.select);
  const search = useListStore((s) => s.search);
  const openSnapshot = useReadingStore((s) => s.snapshot);
  const accounts = useFolderStore((s) => s.accounts);
  const folder = useFolderStore((s) => s.selected);
  const outboxAccount = useFolderStore((s) => s.outboxAccount);
  const filter = useListFilterStore((s) => s.filter);
  const menu = useContextMenu();
  const menuTargets = useRef<MessageRowView[]>([]);

  const outboxMode = outboxAccount !== null;
  const searching = search !== null;
  const shownRows = filteredRows(rows, filter);
  const counts = filterCounts(rows);

  // The folder switch resets the filter to All: a tab named over one
  // folder's rows says nothing about the next folder's.
  useEffect(() => {
    useListFilterStore.getState().reset();
  }, [folder]);

  // Whether the open folder is the account's drafts folder: its rows wear
  // the Draft chip and open the draft menu.
  const draftFolder = (() => {
    if (folder === null) {
      return false;
    }
    const account = accounts.find((a) => a.account_id === folder.account);
    return account?.folders.find((f) => f.key === folder.key)?.role === 'drafts';
  })();

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
  // set the live mode is drawing. The folder switch clears the
  // multi-selection outright (it resets beside the filter).
  const viewRows = searching ? search.rows : shownRows;
  const selectedHidden =
    selectedKey !== null && !viewRows.some((row) => sameRowKey(row.key, selectedKey));
  useEffect(() => {
    if (selectedHidden) {
      useListStore.getState().select(null);
    }
  }, [selectedHidden]);
  useEffect(() => {
    useListStore.getState().setSelection([], null);
  }, [folder]);

  const openRow = (key: RowKey, event: React.MouseEvent): void => {
    const store = useListStore.getState();
    if (event.ctrlKey || event.metaKey) {
      // Ctrl toggles the row in the multi-selection and never opens.
      const next = toggleKey(store.selectedKeys, store.anchorKey, key);
      store.setSelection(next.selection, next.anchor);
      return;
    }
    if (event.shiftKey && store.selectedKeys.length > 0) {
      // Shift ranges from the anchor over the rendered items.
      const items = searching ? flatItems(search.rows) : withDateHeaders(shownRows, now);
      store.setSelection(rangeKeys(items, store.anchorKey, key), store.anchorKey);
      return;
    }
    store.setSelection([key], key);
    select(key);
    // A message row opens the reading pane; a thread row names no single
    // message, so it selects alone until the thread slice lands.
    if ('message' in key) {
      void dispatch(intents.openMessage(key.message));
    }
  };

  // Right-click: the Outlook semantics the selection model owns: inside
  // the multi-selection it stays the targets, outside it the selection
  // collapses to the clicked row before the menu opens.
  const rowMenu = (key: RowKey, event: React.MouseEvent): void => {
    const store = useListStore.getState();
    const targets = resolveTargets(store.selectedKeys, key);
    if (targets.length === 1 && !store.selectedKeys.some((k) => sameRowKey(k, key))) {
      store.setSelection([key], key);
    }
    const pool = searching ? search.rows : rows;
    menuTargets.current = pool.filter((row) => targets.some((key2) => sameRowKey(key2, row.key)));
    menu.open(event);
  };

  // Ctrl/Cmd+A anywhere in the pane selects every loaded row.
  const onKeyDown = (event: React.KeyboardEvent): void => {
    if ((event.ctrlKey || event.metaKey) && event.key === 'a' && !searching && !outboxMode) {
      event.preventDefault();
      useListStore
        .getState()
        .setSelection(allKeys(withDateHeaders(shownRows, now)), null);
    }
  };

  // The bulk bar's rows: the selection resolved against the rows the pane is
  // showing, whichever set that is.
  const bulkRows =
    selectedKeys.length > 0
      ? viewRows.filter((row) => selectedKeys.some((key) => sameRowKey(key, row.key)))
      : null;

  // The account context the menu asks about: the first target's account and
  // its folders, the sitting folder out, and the provider's report
  // capability the spam verb gates on. Undefined only when the pane holds
  // no group for that account. A selection that spans accounts still gets
  // its context, so the move item's own one-account gate hides the verb
  // rather than a placeholder misnaming a shipped feature.
  const accountContextOf = (targets: MessageRowView[]): RowAccountContext | undefined => {
    if (targets.length === 0) {
      return undefined;
    }
    const account = targets[0]!.account_id;
    const group = accounts.find((row) => row.account_id === account);
    if (group === undefined) {
      return undefined;
    }
    return {
      account,
      currentKey: folder !== null && folder.account === account ? folder.key : null,
      canReportMail: group.can_report_mail === true,
      folders: group.folders,
    };
  };

  const now = new Date();

  // The window compares against every row the snapshot sent, not the
  // filtered view: rows behind the load window may match the chosen tab,
  // so a full window keeps offering to grow whatever the filter shows.
  const windowIsFull = visibleLimit > 0 && rows.length === visibleLimit;
  const showRows = folder !== null && rows.length > 0;

  return (
    <section
      aria-label="Message list"
      onKeyDown={onKeyDown}
      className="relative flex h-full min-w-0 flex-1 flex-col overflow-hidden bg-card"
    >
      <ListToolbar
        folderName={folderDisplayName(accounts, folder)}
        viewMode={viewMode}
        filter={filter}
        filterCounts={counts}
        onFilter={(tab) => useListFilterStore.getState().setFilter(tab)}
        search={search}
        outbox={outboxMode}
        bulk={
          bulkRows !== null
            ? {
                rows: bulkRows,
                onSelectAll: () =>
                  useListStore
                    .getState()
                    .setSelection(allKeys(withDateHeaders(shownRows, now)), null),
                onClear: () => useListStore.getState().setSelection([], null),
              }
            : null
        }
      />
      {outboxMode ? (
        <OutboxQueue account={outboxAccount} />
      ) : searching ? (
        search.rows.length === 0 ? (
          <EmptyState glyph={<MailIcon size={24} />} message={STATE_CONTENT.searchEmpty!.message} />
        ) : (
          <RowWindow items={flatItems(search.rows)} highlight={highlight} selectedKeys={selectedKeys} drafts={false} now={now} openRow={openRow} rowMenu={rowMenu} showMore={false} />
        )
      ) : folder === null ? (
        <EmptyState glyph={<MailIcon size={24} />} message={STATE_CONTENT.listNoFolder!.message} />
      ) : !showRows ? (
        <EmptyState glyph={<MailIcon size={24} />} message={STATE_CONTENT.listEmptyFolder!.message} />
      ) : shownRows.length === 0 ? (
        <EmptyState glyph={<MailIcon size={24} />} message={STATE_CONTENT.listEmptyFilter!.message} />
      ) : (
        <RowWindow items={withDateHeaders(shownRows, now)} highlight={highlight} selectedKeys={selectedKeys} drafts={draftFolder} now={now} openRow={openRow} rowMenu={rowMenu} showMore={windowIsFull} />
      )}
      {menu.state !== null && (
        <ContextMenu
          x={menu.state.x}
          y={menu.state.y}
          items={
            draftFolder
              ? buildDraftMenu()
              : buildRowMenu(menuTargets.current, viewMode, accountContextOf(menuTargets.current))
          }
          onClose={menu.close}
        />
      )}
      <SwipeToast />
    </section>
  );
}
