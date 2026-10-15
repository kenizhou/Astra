// SPDX-License-Identifier: GPL-3.0-only
// The folder tree inside one account group: rows are already depth-first with
// parent keys, so nesting is an indent by depth and a visibility walk up the
// parent chain. Expansion and selection cross back to the core as intents; the
// tree keeps no expansion state of its own, which is what keeps it in step with
// the snapshot every pull brings. Each account's group closes with its Outbox
// row, which leads the list's queue view instead of a folder.

import type { ComponentType } from 'react';

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
import { dispatch, intents, type FolderRef } from '@/services/ipc';
import type { AccountFolderRow, FolderRow, FolderRole } from '@/services/surfaces';
import { useFolderStore, visibleFolders } from '@/stores/folderStore';
import { useSendingStore } from '@/stores/sendingStore';

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
}

function FolderTreeRow({ accountId, row, selected }: FolderTreeRowProps) {
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
      className={`relative flex min-h-[var(--row-h)] items-center gap-2 rounded-lg pr-2 text-[13px] transition-colors ${
        selected
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

  const onToggleAccount = () => {
    void dispatch(intents.setAccountExpanded(account.account_id, !account.expanded));
  };

  // A collapsed account still owes its reader the unread total: the rows are
  // hidden, so the header carries the sum until the tree is opened again.
  const collapsedUnread = account.expanded
    ? 0
    : account.folders.reduce((total, row) => total + row.unread, 0);

  return (
    <section aria-label={account.display_name} className="space-y-0.5 pb-1">
      <div className="flex h-9 items-center gap-1 pl-2 pr-2">
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
        {collapsedUnread > 0 && (
          <span className="ml-auto shrink-0 rounded-full bg-primary px-1.5 py-0.5 text-[11px] font-medium tabular-nums leading-none text-primary-foreground">
            {collapsedUnread}
          </span>
        )}
      </div>
      {rows.map((row) => (
        <FolderTreeRow
          key={`${account.account_id}/${row.key}`}
          accountId={account.account_id}
          row={row}
          selected={
            selected !== null &&
            selected.account === account.account_id &&
            selected.key === row.key
          }
        />
      ))}
      {account.expanded && <OutboxTreeRow accountId={account.account_id} />}
    </section>
  );
}
