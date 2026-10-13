// SPDX-License-Identifier: GPL-3.0-only
// The desktop shell: the frameless title bar over the command ribbon and a
// content row, the status bar under it, all coloured through the theme
// tokens. The ribbon stands in the mail view only. The content row is the
// tool rail beside a padded surface holding the active view's cards: the
// mail view is a resizable panel row (folder pane, message list, and reading
// pane, the quick-reply strip docked at the reading pane's bottom, and the
// agent panel docked right when the rail's toggle opens it), the calendar
// and people views fill one card each. Under the compact breakpoint the
// folder panel leaves the row and rides the drawer the rail opens. The New
// message action floats over the panes' corner and opens the pop-out
// composer window, the app start resume opens the stored draft into the
// strip, a landed compose request opens a pop-out window hydrated from it,
// the AI documents panel rides above
// the status bar's corner when the rail's toggle opens it, the
// send-result toast says when a queued send has gone out, the settings
// screen rides above the shell when the title bar's gear opens it, and the
// account setup flow rides above everything when it is open.

import { Group, Panel, Separator } from 'react-resizable-panels';

import { AccountSetupFlow } from '@/features/account-setup/AccountSetupFlow';
import { AiPanel } from '@/features/ai-panel/AiPanel';
import { AgentPanel } from '@/features/agent/AgentPanel';
import { CalendarPane } from '@/features/calendar/CalendarPane';
import { NewMessageAction, useDraftResumeAtStart } from '@/features/composer/Composer';
import { useComposeRequestHydration } from '@/features/composer/useComposeRequest';
import { SendResultToast } from '@/features/outbox/SendResultToast';
import { PeoplePane } from '@/features/people/PeoplePane';
import { SettingsScreen } from '@/features/settings/SettingsScreen';
import { FolderPane } from '@/components/folder-pane/FolderPane';
import { useIsCompact } from '@/hooks/useWindowSize';
import { useAccountStore } from '@/stores/accountStore';
import { useUiStore } from '@/stores/uiStore';
import { CommandRibbon } from '@/components/ribbon/CommandRibbon';
import { FolderPaneDrawer } from './FolderPaneDrawer';
import { MessageList } from './MessageList';
import { ReadingPane } from './ReadingPane';
import { StatusBar } from './StatusBar';
import { TitleBar } from './TitleBar';
import { ToolWindowBar } from './ToolWindowBar';
import { pct } from './panelSizes';

/** One view card: a rounded, subtly bordered surface filling its panel. */
function PanelCard({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl border border-border-subtle bg-card">
      {children}
    </div>
  );
}

/** The pill handle between two panels. */
function VDivider() {
  return (
    <Separator className="mx-1 w-1.5 rounded-full bg-border transition-colors hover:bg-primary" />
  );
}

/** The mail view: a resizable row of folder tree, list, and reading panes,
 *  plus the docked agent column. The composer lives in the reading pane's
 *  quick-reply strip and in pop-out windows now, so the row holds no fourth
 *  column. Every conditional panel joins the Group key, so the layout
 *  remounts cleanly when one arrives or leaves. The default sizes sum to
 *  one hundred in each combination. */
function MailRow({ compact }: { compact: boolean }) {
  const agentOpen = useUiStore((s) => s.agentOpen);

  return (
    <Group
      key={`${agentOpen ? 'agent' : 'solo'}-${compact ? 'compact' : 'wide'}`}
      orientation="horizontal"
      className="h-full"
    >
      {!compact && (
        <Panel id="folder-pane" defaultSize={pct(18)} minSize={pct(12)}>
          <PanelCard>
            <FolderPane />
          </PanelCard>
        </Panel>
      )}
      {!compact && <VDivider />}
      <Panel id="message-list" defaultSize={pct(agentOpen ? 34 : 38)} minSize={pct(25)}>
        <PanelCard>
          <MessageList />
        </PanelCard>
      </Panel>
      <VDivider />
      <Panel id="reading-pane" defaultSize={pct(agentOpen ? 26 : 44)} minSize={pct(20)}>
        <PanelCard>
          <ReadingPane />
        </PanelCard>
      </Panel>
      {agentOpen && <VDivider />}
      {agentOpen && (
        <Panel id="agent-dock" defaultSize={pct(22)} minSize={pct(14)} maxSize={pct(40)}>
          <PanelCard>
            <AgentPanel />
          </PanelCard>
        </Panel>
      )}
    </Group>
  );
}

export function AppShell() {
  const accountCount = useAccountStore((s) => s.accounts.length);
  const shellView = useUiStore((s) => s.shellView);
  const compact = useIsCompact();
  useDraftResumeAtStart();
  useComposeRequestHydration();

  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-background font-ui text-foreground">
      <TitleBar />
      <CommandRibbon />
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <ToolWindowBar />
        <main className="min-w-0 flex-1 bg-surface p-2">
          <div className="relative h-full">
            {shellView === 'people' ? (
              <PanelCard>
                <PeoplePane />
              </PanelCard>
            ) : shellView === 'calendar' ? (
              <PanelCard>
                <CalendarPane />
              </PanelCard>
            ) : (
              <MailRow compact={compact} />
            )}
            {shellView === 'mail' && compact && (
              <FolderPaneDrawer>
                <FolderPane />
              </FolderPaneDrawer>
            )}
          </div>
        </main>
      </div>
      <StatusBar />
      {/* The composer pane mounts only in the mail view, so the action
          stands nowhere over the calendar or people panes. */}
      <NewMessageAction disabled={accountCount === 0 || shellView !== 'mail'} />
      <AiPanel />
      <SendResultToast />
      <SettingsScreen />
      <AccountSetupFlow />
    </div>
  );
}
