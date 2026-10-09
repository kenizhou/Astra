// SPDX-License-Identifier: GPL-3.0-only
// The desktop shell: the frameless title bar over the folder pane, the
// message list, the reading pane, and the composer pane when a composition
// is open, with the status bar under them, all coloured through the theme
// tokens. The New message action floats over the panes' corner, and the
// app start resume opens the stored draft into the composer. The account
// setup flow rides above everything when it is open.

import { AccountSetupFlow } from '@/features/account-setup/AccountSetupFlow';
import { ComposerPane, NewMessageAction, useDraftResumeAtStart } from '@/features/composer/Composer';
import { FolderPane } from '@/components/folder-pane/FolderPane';
import { useAccountStore } from '@/stores/accountStore';
import { MessageList } from './MessageList';
import { ReadingPane } from './ReadingPane';
import { StatusBar } from './StatusBar';
import { TitleBar } from './TitleBar';

export function AppShell() {
  const accountCount = useAccountStore((s) => s.accounts.length);
  useDraftResumeAtStart();

  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-background font-ui text-foreground">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <FolderPane />
        <MessageList />
        <ReadingPane />
        <ComposerPane />
      </div>
      <StatusBar />
      <NewMessageAction disabled={accountCount === 0} />
      <AccountSetupFlow />
    </div>
  );
}
