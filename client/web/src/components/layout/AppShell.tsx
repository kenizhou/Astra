// SPDX-License-Identifier: GPL-3.0-only
// The desktop shell: the frameless title bar over the folder pane, the
// message list, and the reading pane, with the status bar under them, all
// coloured through the theme tokens. The account setup flow rides above
// everything when it is open.

import { AccountSetupFlow } from '@/features/account-setup/AccountSetupFlow';
import { FolderPane } from '@/components/folder-pane/FolderPane';
import { MessageList } from './MessageList';
import { ReadingPane } from './ReadingPane';
import { StatusBar } from './StatusBar';
import { TitleBar } from './TitleBar';

export function AppShell() {
  return (
    <div className="flex h-full flex-col overflow-hidden bg-background font-ui text-foreground">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <FolderPane />
        <MessageList />
        <ReadingPane />
      </div>
      <StatusBar />
      <AccountSetupFlow />
    </div>
  );
}
