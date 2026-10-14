// SPDX-License-Identifier: GPL-3.0-only
// The folder pane: every account's folder tree, straight off the mailbox-list
// snapshot the pulls keep fresh. The pane owns no state of its own beyond the
// scroll; the tree underneath renders what the core last published.

import { FolderTree } from './FolderTree';
import { UnifiedInboxRow } from './UnifiedInboxRow';
import { useFolderStore } from '@/stores/folderStore';

export function FolderPane() {
  const accounts = useFolderStore((s) => s.accounts);

  return (
    <nav
      aria-label="Mail folders"
      className="h-full w-full overflow-y-auto bg-surface py-1"
    >
      {accounts.length === 0 && (
        <p className="px-3 py-2 text-xs text-muted-text">
          No account yet. Add one from the title bar.
        </p>
      )}
      <div role="tree" aria-label="Accounts" className="space-y-1 px-1.5">
        <UnifiedInboxRow accounts={accounts} />
        {accounts.map((account, index) => (
          <FolderTree key={account.account_id} account={account} paletteIndex={index % 8} />
        ))}
      </div>
    </nav>
  );
}
