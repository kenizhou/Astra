// SPDX-License-Identifier: GPL-3.0-only
// The desktop shell: the frameless title bar over the content row the view
// switch picks (the mail trio of the folder pane, the message list, the
// reading pane, and the composer pane when a composition is open, the
// calendar pane, or the people pane), with the status bar under them, all
// coloured through the theme tokens. The New message action floats over the
// panes' corner, the app start resume opens the stored draft into the
// composer, a landed compose request opens the composer hydrated from it,
// the outbox, agent, and AI documents panels ride above the status bar's
// corner when the status bar's toggles open them, the send-result toast
// says when a queued send has gone out, the settings screen rides above
// the shell when the title bar's gear opens it, and the account setup flow
// rides above everything when it is open.

import { AccountSetupFlow } from '@/features/account-setup/AccountSetupFlow';
import { AiPanel } from '@/features/ai-panel/AiPanel';
import { AgentPanel } from '@/features/agent/AgentPanel';
import { CalendarPane } from '@/features/calendar/CalendarPane';
import { ComposerPane, NewMessageAction, useDraftResumeAtStart } from '@/features/composer/Composer';
import { useComposeRequestHydration } from '@/features/composer/useComposeRequest';
import { OutboxPanel } from '@/features/outbox/OutboxPanel';
import { SendResultToast } from '@/features/outbox/SendResultToast';
import { PeoplePane } from '@/features/people/PeoplePane';
import { SettingsScreen } from '@/features/settings/SettingsScreen';
import { FolderPane } from '@/components/folder-pane/FolderPane';
import { useAccountStore } from '@/stores/accountStore';
import { useUiStore } from '@/stores/uiStore';
import { MessageList } from './MessageList';
import { ReadingPane } from './ReadingPane';
import { StatusBar } from './StatusBar';
import { TitleBar } from './TitleBar';

/** The mail content row: folder tree, list, reading pane, composer. */
function MailRow() {
  return (
    <div className="flex min-h-0 flex-1">
      <FolderPane />
      <MessageList />
      <ReadingPane />
      <ComposerPane />
    </div>
  );
}

export function AppShell() {
  const accountCount = useAccountStore((s) => s.accounts.length);
  const shellView = useUiStore((s) => s.shellView);
  useDraftResumeAtStart();
  useComposeRequestHydration();

  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-background font-ui text-foreground">
      <TitleBar />
      {shellView === 'people' ? (
        <PeoplePane />
      ) : shellView === 'calendar' ? (
        <CalendarPane />
      ) : (
        <MailRow />
      )}
      <StatusBar />
      {/* The composer pane mounts only in the mail view, so the action
          stands nowhere over the calendar or people panes. */}
      <NewMessageAction disabled={accountCount === 0 || shellView !== 'mail'} />
      <OutboxPanel />
      <AgentPanel />
      <AiPanel />
      <SendResultToast />
      <SettingsScreen />
      <AccountSetupFlow />
    </div>
  );
}
