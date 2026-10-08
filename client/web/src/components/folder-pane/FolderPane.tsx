// SPDX-License-Identifier: GPL-3.0-only
// The folder pane: every account's folder tree, straight off the mailbox-list
// snapshot the pulls keep fresh. The pane owns no state of its own beyond the
// scroll; the tree underneath renders what the core last published.

import { FolderTree } from './FolderTree';
import { useFolderStore } from '@/stores/folderStore';

export function FolderPane() {
  const accounts = useFolderStore((s) => s.accounts);

  return (
    <nav
      aria-label="Mail folders"
      className="w-56 shrink-0 overflow-y-auto border-r border-border bg-surface py-1"
    >
      {accounts.length === 0 && (
        <p className="px-3 py-2 text-xs text-muted-foreground">
          No account yet. Add one from the title bar.
        </p>
      )}
      <div role="tree" aria-label="Accounts">
        {accounts.map((account) => (
          <FolderTree key={account.account_id} account={account} />
        ))}
      </div>
    </nav>
  );
}
