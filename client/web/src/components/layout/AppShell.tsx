// SPDX-License-Identifier: GPL-3.0-only
// The desktop shell: the frameless title bar over the folder pane, the
// message list, the reading pane, and the composer pane when a composition
// is open, with the status bar under them, all coloured through the theme
// tokens. The New message action floats over the panes' corner, the app
// start resume opens the stored draft into the composer, a landed compose
// request opens the composer hydrated from it, the outbox panel rides above
// the status bar's corner when the status bar's toggle opens it, and the
// send-result toast says when a queued send has gone out. The account setup
// flow rides above everything when it is open.

import { AccountSetupFlow } from '@/features/account-setup/AccountSetupFlow';
import { ComposerPane, NewMessageAction, useDraftResumeAtStart } from '@/features/composer/Composer';
import { useComposeRequestHydration } from '@/features/composer/useComposeRequest';
import { OutboxPanel } from '@/features/outbox/OutboxPanel';
import { SendResultToast } from '@/features/outbox/SendResultToast';
import { FolderPane } from '@/components/folder-pane/FolderPane';
import { useAccountStore } from '@/stores/accountStore';
import { MessageList } from './MessageList';
import { ReadingPane } from './ReadingPane';
import { StatusBar } from './StatusBar';
import { TitleBar } from './TitleBar';

export function AppShell() {
  const accountCount = useAccountStore((s) => s.accounts.length);
  useDraftResumeAtStart();
  useComposeRequestHydration();

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
      <OutboxPanel />
      <SendResultToast />
      <AccountSetupFlow />
    </div>
  );
}
