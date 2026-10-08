// SPDX-License-Identifier: GPL-3.0-only
// The folder tree inside one account group: rows are already depth-first with
// parent keys, so nesting is an indent by depth and a visibility walk up the
// parent chain. Expansion and selection cross back to the core as intents; the
// tree keeps no expansion state of its own, which is what keeps it in step with
// the snapshot every pull brings.

import { dispatch, intents, type FolderRef } from '@/services/ipc';
import type { AccountFolderRow, FolderRow, FolderRole } from '@/services/surfaces';
import { useFolderStore, visibleFolders } from '@/stores/folderStore';

/** One glyph per role, drawn from the font rather than an icon set for now. */
const ROLE_GLYPH: Record<FolderRole, string> = {
  inbox: '\u2709',
  drafts: '\u270E',
  sent: '\u27A4',
  archive: '\u25A4',
  junk: '\u2298',
  trash: '\u232B',
  other: '\u25A7',
};

const LEAF_GLYPH = '\u25AA';

function roleGlyph(row: FolderRow): string {
  return row.role === undefined ? LEAF_GLYPH : ROLE_GLYPH[row.role];
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
      className={`flex h-[var(--row-h)] items-center gap-1.5 pr-2 text-[13px] transition-colors ${
        selected ? 'bg-selected text-[var(--selected-text)]' : 'text-foreground hover:bg-hover'
      }`}
      style={{ paddingLeft: `${6 + row.depth * 14}px` }}
    >
      {row.has_children ? (
        <button
          type="button"
          onClick={onToggle}
          aria-label={row.expanded ? `Collapse ${row.name}` : `Expand ${row.name}`}
          className="flex h-4 w-4 shrink-0 items-center justify-center rounded-[2px] text-muted-foreground hover:bg-hover hover:text-foreground"
        >
          {row.expanded ? '\u25BE' : '\u25B8'}
        </button>
      ) : (
        <span className="h-4 w-4 shrink-0" aria-hidden="true" />
      )}
      <span aria-hidden="true" className="w-4 shrink-0 text-center text-muted-foreground">
        {roleGlyph(row)}
      </span>
      <button type="button" onClick={onSelect} className="min-w-0 flex-1 truncate text-left">
        {row.name}
      </button>
      {row.unread > 0 && (
        <span className="shrink-0 rounded-full bg-primary px-1.5 text-[11px] font-medium tabular-nums text-primary-foreground">
          {row.unread}
        </span>
      )}
    </div>
  );
}

/** One account group: its header row and the folder rows beneath it. */
export function FolderTree({ account }: { account: AccountFolderRow }) {
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
    <section aria-label={account.display_name}>
      <div className="flex h-[var(--row-h)] items-center gap-1 pr-2 pl-1">
        <button
          type="button"
          onClick={onToggleAccount}
          aria-label={
            account.expanded ? `Collapse ${account.display_name}` : `Expand ${account.display_name}`
          }
          aria-expanded={account.expanded}
          className="flex h-4 w-4 shrink-0 items-center justify-center rounded-[2px] text-muted-foreground hover:bg-hover hover:text-foreground"
        >
          {account.expanded ? '\u25BE' : '\u25B8'}
        </button>
        <span className="truncate text-[var(--text-pane-title)] text-muted-foreground">
          {account.display_name}
        </span>
        {collapsedUnread > 0 && (
          <span className="ml-auto shrink-0 rounded-full bg-primary px-1.5 text-[11px] font-medium tabular-nums text-primary-foreground">
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
    </section>
  );
}
