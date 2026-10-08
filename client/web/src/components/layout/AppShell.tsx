// SPDX-License-Identifier: GPL-3.0-only
// The desktop shell: the frameless title bar over the folder pane and the
// message list, with the status bar under them, all coloured through the theme
// tokens. The message list is a placeholder until the list surface lands; the
// account setup flow rides above everything when it is open.

import { AccountSetupFlow } from '@/features/account-setup/AccountSetupFlow';
import { FolderPane } from '@/components/folder-pane/FolderPane';
import { useFolderStore } from '@/stores/folderStore';
import { StatusBar } from './StatusBar';
import { TitleBar } from './TitleBar';

/** The placeholder for the message list surface that lands next. */
function MessageListPlaceholder() {
  const selected = useFolderStore((s) => s.selected);

  return (
    <section
      aria-label="Message list"
      className="min-w-0 flex-1 overflow-y-auto bg-background p-4 text-sm text-muted-foreground"
    >
      {selected === null ? 'Select a folder to read its mail.' : `Mail in ${selected.key} lands here.`}
    </section>
  );
}

export function AppShell() {
  return (
    <div className="flex h-full flex-col overflow-hidden bg-background font-ui text-foreground">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <FolderPane />
        <MessageListPlaceholder />
      </div>
      <StatusBar />
      <AccountSetupFlow />
    </div>
  );
}
