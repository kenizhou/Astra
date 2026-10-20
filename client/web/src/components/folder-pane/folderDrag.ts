// SPDX-License-Identifier: GPL-3.0-only
// The folder drag's own arithmetic: what one dragged folder may legally land
// on, computed over the account's rows before anything dispatches, so an
// illegal target (a drop into the folder's own subtree, a cross-account drop,
// the unified row) is refused before a single intent crosses. Pure functions;
// the tree's own handlers stay thin.

import type { FolderRow, RowKey } from '@/services/surfaces';
import type { MessageRef } from '@/services/ipc';

/** What the dragged folder was dropped on. */
export type FolderDropTarget =
  | { kind: 'folder'; account: string; key: string }
  | { kind: 'account'; account: string }
  | 'unified';

/** What a legal drop does; `refuse` is the answer an illegal target gets
 *  before anything dispatches. */
export type FolderDropOutcome = 'reparent' | 'make-root' | 'refuse';

/** The keys on the parent chain above `key`, `key` itself excluded. */
function ancestors(key: string, rows: FolderRow[]): Set<string> {
  const byKey = new Map(rows.map((row) => [row.key, row]));
  const chain = new Set<string>();
  let parent = byKey.get(key)?.parent;
  while (parent !== undefined && !chain.has(parent)) {
    chain.add(parent);
    parent = byKey.get(parent)?.parent;
  }
  return chain;
}

/** What dropping `dragged` on `target` does. A folder may reparent onto any
 *  folder of its own account except itself or anything inside its own
 *  subtree, and may become a root on its own account row; everything else,
 *  the unified row included, refuses. */
export function folderDrop(
  dragged: { account: string; key: string },
  target: FolderDropTarget,
  rows: FolderRow[],
): FolderDropOutcome {
  if (target === 'unified' || target.account !== dragged.account) {
    return 'refuse';
  }
  if (target.kind === 'account') {
    return 'make-root';
  }
  if (target.key === dragged.key) {
    return 'refuse';
  }
  // The descendant guard: the target's own chain must not pass through the
  // dragged folder, or the tree would land inside itself.
  if (ancestors(target.key, rows).has(dragged.key)) {
    return 'refuse';
  }
  return 'reparent';
}

/** Whether `key` is the account's Trash or filed inside it: the folder whose
 *  menu offers the permanent delete. */
export function insideTrash(key: string, rows: FolderRow[]): boolean {
  const trash = rows.find((row) => row.role === 'trash')?.key;
  return trash !== undefined && (key === trash || ancestors(key, rows).has(trash));
}

/** The message keys among `keys`, thread keys dropped: the move batch's own
 *  constraint, one message per row. */
export function messageRefsOf(keys: RowKey[]): MessageRef[] {
  return keys.flatMap((key) => ('message' in key ? [key.message] : []));
}
