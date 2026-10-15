// SPDX-License-Identifier: GPL-3.0-only
// The desktop shell: the frameless title bar spans the top, the status bar
// spans the bottom, and the middle row is the tool rail beside the working
// frame: one bordered div per view, its corners carved, a half-step right
// margin off the window's edge. Inside, every view speaks the calendar's
// interior language: the ribbon or toolbar spans the frame's width, and the
// panes ride as rounded cards in a padded row beneath it (the mail view:
// folder pane, message list, and reading pane; the agent panel docked right
// when the rail's toggle opens it). Under the compact breakpoint the folder
// pane leaves the row and rides the drawer the rail opens. Compose starts
// from the ribbon's New Email, which opens the pop-out composer window; the
// app start resume opens the stored draft into the strip, a landed compose
// request opens a pop-out window hydrated from it, the AI documents panel
// rides above the status bar's corner when the rail's toggle opens it, the
// send-result toast says when a queued send has gone out, the settings
// screen rides above the shell when the title bar's gear opens it, and the
// account setup flow rides above everything when it is open.

import { Group, Panel } from 'react-resizable-panels';

import { AccountSetupFlow } from '@/features/account-setup/AccountSetupFlow';
import { AiPanel } from '@/features/ai-panel/AiPanel';
import { AgentPanel } from '@/features/agent/AgentPanel';
import { CalendarPane } from '@/features/calendar/CalendarPane';
import { useDraftResumeAtStart } from '@/features/composer/Composer';
import { useComposeRequestHydration } from '@/features/composer/useComposeRequest';
import { SendResultToast } from '@/features/outbox/SendResultToast';
import { PeoplePane } from '@/features/people/PeoplePane';
import { SettingsScreen } from '@/features/settings/SettingsScreen';
import { FolderPane } from '@/components/folder-pane/FolderPane';
import { useIsCompact } from '@/hooks/useWindowSize';
import { useUiStore } from '@/stores/uiStore';
import { CommandRibbon } from '@/components/ribbon/CommandRibbon';
import { PaneDivider } from './paneDivider';
import { FolderPaneDrawer } from './FolderPaneDrawer';
import { MessageList } from './MessageList';
import { ReadingPane } from './ReadingPane';
import { StatusBar } from './StatusBar';
import { TitleBar } from './TitleBar';
import { ToolWindowBar } from './ToolWindowBar';
import { pct } from './panelSizes';

/** One view filling the frame, no card chrome of its own. */
function FlushView({ children }: { children: React.ReactNode }) {
  return <div className="flex h-full flex-col overflow-hidden">{children}</div>;
}

/** One pane card: a rounded, subtly bordered surface filling its panel,
 *  the calendar view's interior language. */
function PaneCard({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl border border-border-subtle bg-card">
      {children}
    </div>
  );
}

/** The reading pane's card: the same chrome on its paper surface. */
function PaperCard({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl border border-border-subtle bg-[var(--paper)]">
      {children}
    </div>
  );
}

/** The mail view's pane row: the message list, the reading pane, and the
 *  docked agent column, each a card, pill handles between them. The
 *  composer lives in the reading pane's quick-reply strip and in pop-out
 *  windows now, so the row holds no fourth column. The agent dock joins
 *  the Group key, so the layout remounts cleanly when it arrives or
 *  leaves. The default sizes sum to one hundred in each combination. */
function MailRow() {
  const agentOpen = useUiStore((s) => s.agentOpen);

  return (
    <Group
      key={agentOpen ? 'agent' : 'solo'}
      orientation="horizontal"
      className="h-full"
    >
      <Panel id="message-list" defaultSize={pct(agentOpen ? 38 : 47)} minSize={pct(25)}>
        <PaneCard>
          <MessageList />
        </PaneCard>
      </Panel>
      <PaneDivider />
      <Panel id="reading-pane" defaultSize={pct(agentOpen ? 34 : 53)} minSize={pct(20)}>
        <PaperCard>
          <ReadingPane />
        </PaperCard>
      </Panel>
      {agentOpen && <PaneDivider />}
      {agentOpen && (
        <Panel id="agent-dock" defaultSize={pct(28)} minSize={pct(14)} maxSize={pct(40)}>
          <PaneCard>
            <AgentPanel />
          </PaneCard>
        </Panel>
      )}
    </Group>
  );
}

export function AppShell() {
  const shellView = useUiStore((s) => s.shellView);
  const compact = useIsCompact();
  useDraftResumeAtStart();
  useComposeRequestHydration();

  const viewContent = (
    <div className="relative h-full">
      {shellView === 'people' ? (
        <FlushView>
          <PeoplePane />
        </FlushView>
      ) : shellView === 'calendar' ? (
        <FlushView>
          <CalendarPane />
        </FlushView>
      ) : (
        <MailRow />
      )}
      {shellView === 'mail' && compact && (
        <FolderPaneDrawer>
          <FolderPane />
        </FolderPaneDrawer>
      )}
    </div>
  );

  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-background font-ui text-foreground">
      <TitleBar />
      <div className="flex min-h-0 flex-1 bg-chrome">
        <ToolWindowBar />
        {/* The working frame, shared by every view: a faint rule, all
            four corners carved, a half-step right margin standing it off
            the window's edge. */}
        <div className="mr-1 min-w-0 flex-1 overflow-hidden rounded-xl border border-border-subtle">
          {shellView === 'mail' && !compact ? (
            /* The calendar's interior language: the ribbon spans the
               frame's full width, and the panes ride as cards in a padded
               row beneath it, the folder pane one card among them. */
            <div className="flex h-full min-w-0 flex-1 flex-col overflow-hidden bg-background">
              <CommandRibbon />
              <div className="min-h-0 flex-1 p-2">
                <Group orientation="horizontal" className="h-full">
                  <Panel id="folder-pane" defaultSize={pct(18)} minSize={pct(12)} maxSize={pct(36)}>
                    <PaneCard>
                      <FolderPane />
                    </PaneCard>
                  </Panel>
                  <PaneDivider />
                  <Panel id="mail-right" defaultSize={pct(82)} minSize={pct(40)}>
                    <MailRow />
                  </Panel>
                </Group>
              </div>
            </div>
          ) : (
            <div className="flex h-full min-w-0 flex-1 flex-col overflow-hidden">
              <CommandRibbon />
              <main
                className={
                  shellView === 'mail'
                    ? 'min-h-0 min-w-0 flex-1 p-2'
                    : 'min-h-0 min-w-0 flex-1 bg-background'
                }
              >
                {viewContent}
              </main>
            </div>
          )}
        </div>
      </div>
      <StatusBar />
      <AiPanel />
      <SendResultToast />
      <SettingsScreen />
      <AccountSetupFlow />
    </div>
  );
}
