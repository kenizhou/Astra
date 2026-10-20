// SPDX-License-Identifier: GPL-3.0-only
// The folder tree inside one account group: rows are already depth-first with
// parent keys, so nesting is an indent by depth and a visibility walk up the
// parent chain. Expansion and selection cross back to the core as intents; the
// tree keeps no expansion state of its own, which is what keeps it in step with
// the snapshot every pull brings. Each account's group closes with its Outbox
// row, which leads the list's queue view instead of a folder. Right-clicking a
// row opens its context menu; the menu's New Subfolder and Rename stand the
// tree's own inline edit rows. Folders drag: onto a same-account folder to
// reparent, onto the account row to become a root, with the illegal targets
// refusing before anything dispatches; message rows dropped from the list move
// onto the folder they land on.

import { useState, type ComponentType } from 'react';

import {
  ArchiveIcon,
  DraftsIcon,
  FolderIcon,
  InboxIcon,
  JunkIcon,
  OutboxIcon,
  SentIcon,
  TrashIcon,
  type IconProps,
} from '@/components/icons';
import { ContextMenu } from '@/components/ui/ContextMenu';
import { useContextMenu } from '@/components/ui/useContextMenu';
import { dispatch, intents, type FolderRef } from '@/services/ipc';
import type { AccountFolderRow, FolderRow, FolderRole, RowKey } from '@/services/surfaces';
import { useSettingsStore } from '@/features/settings/settingsStore';
import { useFolderStore, visibleFolders } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useSendingStore } from '@/stores/sendingStore';
import { useUiStore } from '@/stores/uiStore';
import { buildFolderMenu } from './folderMenu';
import { folderDrop, messageRefsOf, type FolderDropTarget } from './folderDrag';
import { CONVERSATIONS_DRAG_TYPE, ROWS_DRAG_TYPE } from '@/components/message-list/MessageRow';
import { NewFolderRow } from './NewFolderRow';
import { RenameFolderRow } from './RenameFolderRow';

/** The dataTransfer type a folder drag carries: the dragged folder's ref,
 *  keys only, never message content. */
const FOLDER_DRAG_TYPE = 'application/x-astra-folder';

/** One icon per role; a folder with no role draws the plain folder icon. */
const ROLE_ICON: Record<FolderRole, ComponentType<IconProps>> = {
  inbox: InboxIcon,
  drafts: DraftsIcon,
  sent: SentIcon,
  archive: ArchiveIcon,
  junk: JunkIcon,
  trash: TrashIcon,
  other: FolderIcon,
};

function RoleIcon({ row }: { row: FolderRow }) {
  const Icon = row.role === undefined ? FolderIcon : ROLE_ICON[row.role];
  return <Icon size={15} />;
}

interface FolderTreeRowProps {
  accountId: string;
  row: FolderRow;
  selected: boolean;
  /** Whether the drag hovering this row is a legal target: the highlight. */
  dropTarget: boolean;
  onMenu: (row: FolderRow, event: React.MouseEvent) => void;
  onDragStart: (row: FolderRow, event: React.DragEvent) => void;
  onDragEnd: () => void;
  onDragOver: (row: FolderRow, event: React.DragEvent) => void;
  onDrop: (row: FolderRow, event: React.DragEvent) => void;
}

function FolderTreeRow({
  accountId,
  row,
  selected,
  dropTarget,
  onMenu,
  onDragStart,
  onDragEnd,
  onDragOver,
  onDrop,
}: FolderTreeRowProps) {
  const ref: FolderRef = { account: accountId, key: row.key };

  const onSelect = () => {
    useFolderStore.getState().select(ref);
    void dispatch(intents.selectFolder(ref));
  };

  const onToggle = () => {
    void dispatch(intents.setFolderExpanded(ref, !row.expanded));
  };

  return (
    <div
      role="treeitem"
      aria-selected={selected}
      aria-expanded={row.has_children ? row.expanded : undefined}
      aria-level={row.depth + 1}
      data-depth={row.depth}
      draggable
      onDragStart={(event) => onDragStart(row, event)}
      onDragEnd={onDragEnd}
      onDragOver={(event) => onDragOver(row, event)}
      onDrop={(event) => onDrop(row, event)}
      onContextMenu={(event) => {
        event.preventDefault();
        onMenu(row, event);
      }}
      className={`relative flex min-h-[var(--row-h)] items-center gap-2 rounded-lg pr-2 text-[13px] transition-colors ${
        dropTarget
          ? 'bg-selected text-[var(--selected-text)] outline-dashed outline-1 outline-ring'
          : selected
            ? 'bg-selected font-medium text-[var(--selected-text)]'
            : 'text-foreground hover:bg-hover'
      }`}
      style={{ paddingLeft: `${12 + row.depth * 14}px` }}
    >
      {row.has_children ? (
        <button
          type="button"
          onClick={onToggle}
          aria-label={row.expanded ? `Collapse ${row.name}` : `Expand ${row.name}`}
          className="flex min-w-[var(--hit)] shrink-0 items-center justify-center self-stretch rounded-[2px] text-muted-text hover:bg-hover hover:text-foreground"
        >
          {row.expanded ? '▾' : '▸'}
        </button>
      ) : (
        <span className="h-4 w-4 shrink-0" aria-hidden="true" />
      )}
      <span aria-hidden="true" className="flex w-4 shrink-0 items-center justify-center text-muted-text">
        <RoleIcon row={row} />
      </span>
      <button type="button" onClick={onSelect} className="min-w-0 flex-1 truncate text-left">
        {row.name}
      </button>
      {row.unread > 0 && (
        <span className="shrink-0 rounded-full bg-primary px-1.5 py-0.5 text-[11px] font-medium tabular-nums leading-none text-primary-foreground">
          {row.unread}
        </span>
      )}
    </div>
  );
}

/** The account's Outbox row: leads the list's queue view, badging the sends
 * still owed to the server while any stand. */
function OutboxTreeRow({ accountId }: { accountId: string }) {
  const selected = useFolderStore((s) => s.outboxAccount === accountId);
  const pending = useSendingStore(
    (s) =>
      s.snapshot.rows.filter(
        (row) =>
          row.queued.account === accountId && (row.state === 'queued' || row.state === 'sending'),
      ).length,
  );

  const onSelect = () => {
    useFolderStore.getState().selectOutbox(accountId);
    void dispatch(intents.showOutbox());
  };

  return (
    <div
      role="treeitem"
      aria-selected={selected}
      aria-level={1}
      className={`relative flex min-h-[var(--row-h)] items-center gap-2 rounded-lg pl-3 pr-2 text-[13px] transition-colors ${
        selected
          ? 'bg-selected font-medium text-[var(--selected-text)]'
          : 'text-foreground hover:bg-hover'
      }`}
    >
      <span className="h-4 w-4 shrink-0" aria-hidden="true" />
      <span aria-hidden="true" className="flex w-4 shrink-0 items-center justify-center text-muted-text">
        <OutboxIcon size={15} />
      </span>
      <button type="button" onClick={onSelect} className="min-w-0 flex-1 truncate text-left">
        Outbox
      </button>
      {pending > 0 && (
        <span className="shrink-0 rounded-full bg-primary px-1.5 py-0.5 text-[11px] font-medium tabular-nums leading-none text-primary-foreground">
          {pending}
        </span>
      )}
    </div>
  );
}

/** The per-account refusal badge: a Microsoft account whose calendar the
 * grant withholds, named on its group header. One tap opens the accounts
 * page, where the reconnect banner offers the fresh sign-in. */
function RefusalBadge({ accountId, displayName }: { accountId: string; displayName: string }) {
  const grant = useSettingsStore(
    (s) => s.accounts.find((row) => row.account === accountId)?.grant,
  );

  if (grant !== 'calendar_withheld') {
    return null;
  }

  const onReconnect = () => {
    useSettingsStore.getState().requestCategory('accounts');
    useUiStore.getState().setSettingsOpen(true);
  };

  return (
    <button
      type="button"
      aria-label={`Reconnect ${displayName}`}
      title="The calendar permission is missing; reconnect this account"
      onClick={onReconnect}
      className="ml-auto shrink-0 rounded-full border border-[var(--warning)]/50 px-1.5 py-0.5 text-[10px] leading-none text-[var(--warning)] transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      calendar
    </button>
  );
}

/** One account group: its header row, the folder rows beneath it, and the
 * Outbox row closing the group. */
export function FolderTree({
  account,
  paletteIndex,
}: {
  account: AccountFolderRow;
  paletteIndex: number;
}) {
  const selected = useFolderStore((s) => s.selected);
  const rows = visibleFolders(account);
  const menu = useContextMenu();
  const [menuRow, setMenuRow] = useState<FolderRow | null>(null);
  const [creatingUnder, setCreatingUnder] = useState<FolderRow | null>(null);
  const [renaming, setRenaming] = useState<FolderRow | null>(null);
  // The folder this tree is dragging, held beside the dataTransfer because a
  // dragover cannot read the payload, only its type.
  const [dragging, setDragging] = useState<FolderRef | null>(null);
  const [dropOn, setDropOn] = useState<string | null>(null);

  const onToggleAccount = () => {
    void dispatch(intents.setAccountExpanded(account.account_id, !account.expanded));
  };

  const openMenu = (row: FolderRow, event: React.MouseEvent): void => {
    setMenuRow(row);
    menu.open(event);
  };

  // The inline create asks its parent to stand open first: the edit row
  // rides the parent's visibility chain like every child row.
  const startCreate = (row: FolderRow): void => {
    if (row.has_children && !row.expanded) {
      void dispatch(
        intents.setFolderExpanded({ account: account.account_id, key: row.key }, true),
      );
    }
    setCreatingUnder(row);
  };

  // The folder's listed rows, when this folder is the one the list shows:
  // what Mark All as Read loops over.
  const listedRowsOf = (row: FolderRow) =>
    selected !== null &&
    selected.account === account.account_id &&
    selected.key === row.key
      ? useListStore.getState().rows
      : null;

  // A collapsed account still owes its reader the unread total: the rows are
  // hidden, so the header carries the sum until the tree is opened again.
  const collapsedUnread = account.expanded
    ? 0
    : account.folders.reduce((total, row) => total + row.unread, 0);

  const onFolderDragStart = (row: FolderRow, event: React.DragEvent): void => {
    if (account.can_edit_folders !== true) {
      event.preventDefault();
      return;
    }
    const ref = { account: account.account_id, key: row.key };
    event.dataTransfer.setData(FOLDER_DRAG_TYPE, JSON.stringify(ref));
    event.dataTransfer.effectAllowed = 'move';
    setDragging(ref);
  };

  /** Whether the hover is a target the standing drag may legally land on. */
  const legalTarget = (target: FolderDropTarget): boolean => {
    if (dragging !== null) {
      return folderDrop(dragging, target, account.folders) !== 'refuse';
    }
    // A message drag: any folder of the drag's own account takes it, and the
    // drop itself keeps only the rows that belong here.
    return target !== 'unified';
  };

  const onRowDragOver = (row: FolderRow, event: React.DragEvent): void => {
    if (
      dragging === null &&
      !event.dataTransfer.types.includes(ROWS_DRAG_TYPE)
    ) {
      return;
    }
    // A selection holding a conversation declared it at the source: the
    // hover refuses what the drop could not name, so no highlight invites a
    // drop that would do nothing.
    if (event.dataTransfer.types.includes(CONVERSATIONS_DRAG_TYPE)) {
      return;
    }
    const target: FolderDropTarget = {
      kind: 'folder',
      account: account.account_id,
      key: row.key,
    };
    if (legalTarget(target)) {
      event.preventDefault();
      event.dataTransfer.dropEffect = 'move';
      setDropOn(row.key);
    }
  };

  const onRowDrop = (row: FolderRow, event: React.DragEvent): void => {
    const target: FolderDropTarget = {
      kind: 'folder',
      account: account.account_id,
      key: row.key,
    };
    if (dragging !== null) {
      const outcome = folderDrop(dragging, target, account.folders);
      setDragging(null);
      setDropOn(null);
      if (outcome === 'reparent') {
        void dispatch(intents.moveFolder(account.account_id, dragging.key, row.key));
      }
      return;
    }
    // A message drop moves the payload only when every key names a message:
    // a conversation the batch cannot name refuses the whole drop, the same
    // refusal the hover already gave it; the rest keep to their account.
    const keys = droppedRowKeys(event);
    if (keys.length > 0 && keys.every((key) => 'message' in key)) {
      const refs = messageRefsOf(keys);
      void dispatch(
        intents.moveMessages(
          refs.filter((ref) => ref.account === account.account_id),
          { account: account.account_id, key: row.key },
        ),
      );
    }
    setDropOn(null);
  };

  const onAccountDragOver = (event: React.DragEvent): void => {
    if (dragging === null) {
      return;
    }
    if (folderDrop(dragging, { kind: 'account', account: account.account_id }, account.folders) !== 'refuse') {
      event.preventDefault();
      event.dataTransfer.dropEffect = 'move';
      setDropOn('account');
    }
  };

  const onAccountDrop = (): void => {
    if (dragging === null) {
      return;
    }
    void dispatch(intents.moveFolder(account.account_id, dragging.key));
    setDragging(null);
    setDropOn(null);
  };

  return (
    <section aria-label={account.display_name} className="space-y-0.5 pb-1">
      <div
        className={`flex h-9 items-center gap-1 pl-2 pr-2 ${dropOn === 'account' ? 'rounded-lg bg-selected' : ''}`}
        onDragOver={onAccountDragOver}
        onDragLeave={() => setDropOn((current) => (current === 'account' ? null : current))}
        onDrop={onAccountDrop}
      >
        <button
          type="button"
          onClick={onToggleAccount}
          aria-label={
            account.expanded ? `Collapse ${account.display_name}` : `Expand ${account.display_name}`
          }
          aria-expanded={account.expanded}
          className="flex min-w-[var(--hit)] shrink-0 items-center justify-center self-stretch rounded-[2px] text-muted-text hover:bg-hover hover:text-foreground"
        >
          {account.expanded ? '▾' : '▸'}
        </button>
        <span
          aria-hidden="true"
          data-account-dot
          className="h-2 w-2 shrink-0 rounded-full"
          style={{ background: `var(--account-${paletteIndex + 1})` }}
        />
        <span className="truncate text-[11px] uppercase tracking-wide text-muted-text">
          {account.display_name}
        </span>
        <RefusalBadge accountId={account.account_id} displayName={account.display_name} />
        {collapsedUnread > 0 && (
          <span className="ml-auto shrink-0 rounded-full bg-primary px-1.5 py-0.5 text-[11px] font-medium tabular-nums leading-none text-primary-foreground">
            {collapsedUnread}
          </span>
        )}
      </div>
      {rows.map((row) => (
        <div key={`${account.account_id}/${row.key}`}>
          <FolderTreeRow
            accountId={account.account_id}
            row={row}
            selected={
              selected !== null &&
              selected.account === account.account_id &&
              selected.key === row.key
            }
            dropTarget={dropOn === row.key}
            onMenu={openMenu}
            onDragStart={onFolderDragStart}
            onDragEnd={() => {
              setDragging(null);
              setDropOn(null);
            }}
            onDragOver={onRowDragOver}
            onDrop={onRowDrop}
          />
          {creatingUnder !== null && creatingUnder.key === row.key && (
            <NewFolderRow
              account={account.account_id}
              parent={row.key}
              depth={row.depth + 1}
              onDone={() => setCreatingUnder(null)}
            />
          )}
          {renaming !== null && renaming.key === row.key && (
            <RenameFolderRow
              account={account.account_id}
              folder={row.key}
              name={row.name}
              depth={row.depth}
              onDone={() => setRenaming(null)}
            />
          )}
        </div>
      ))}
      {account.expanded && <OutboxTreeRow accountId={account.account_id} />}
      {menu.state !== null && menuRow !== null && (
        <ContextMenu
          x={menu.state.x}
          y={menu.state.y}
          items={buildFolderMenu(menuRow, account, {
            onCreateSubfolder: () => startCreate(menuRow),
            onRename: () => setRenaming(menuRow),
            listedRows: listedRowsOf(menuRow),
          })}
          onClose={menu.close}
        />
      )}
    </section>
  );
}

/** The row keys a dropped selection carried, keys only, thread keys
 *  included: the drop site decides what a batch can name. */
function droppedRowKeys(event: React.DragEvent): RowKey[] {
  try {
    const raw = event.dataTransfer.getData(ROWS_DRAG_TYPE);
    if (raw === '') {
      return [];
    }
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed as RowKey[];
  } catch {
    return [];
  }
}
