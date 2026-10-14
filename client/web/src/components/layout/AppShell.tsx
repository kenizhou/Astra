// SPDX-License-Identifier: GPL-3.0-only
// The desktop shell: the frameless title bar spans the top, the status bar
// spans the bottom, and the middle row is the left column (the tool rail
// beside the folder pane in the mail view) next to the right zone (the
// command ribbon over the content). The right zone rounds its two left
// corners into the column, so the junctions read as one carved surface.
// The ribbon stands in the mail view only. The content is the active view:
// the mail view is a resizable panel row (message list and reading pane,
// the quick-reply strip docked at the reading pane's bottom, and the agent
// panel docked right when the rail's toggle opens it), the calendar and
// people views fill one card each. Under the compact breakpoint the folder
// pane leaves the column and rides the drawer the rail opens. Compose
// starts from the ribbon's New Email, which opens the pop-out composer
// window; the app start resume opens the stored draft into the strip, a
// landed compose request opens a pop-out window hydrated from it, the AI
// documents panel rides above the status bar's corner when the rail's
// toggle opens it, the send-result toast says when a queued send has gone
// out, the settings screen rides above the shell when the title bar's gear
// opens it, and the account setup flow rides above everything when it is
// open.

import { Group, Panel, Separator } from 'react-resizable-panels';

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

/** The pane divider: the A3 hairline between two panes. The grab zone
 *  reaches three pixels to either side invisibly; hover lifts a quiet wash
 *  of the ink colour over the line, never the accent, so a drag reads as
 *  the structure yielding rather than a control lighting up. */
function VDivider() {
  return (
    <Separator className="group relative w-px shrink-0 cursor-col-resize bg-[var(--hairline)]">
      <span aria-hidden="true" className="absolute inset-y-0 -right-[3px] -left-[3px]" />
      <span
        aria-hidden="true"
        className="absolute inset-y-0 left-1/2 w-[3px] -translate-x-1/2 rounded-full bg-[var(--muted-text)] opacity-0 transition-opacity duration-[var(--dur-row)] group-hover:opacity-30"
      />
    </Separator>
  );
}

/** The mail view: a resizable row of the message list and the reading
 *  pane, plus the docked agent column, flush against one another with no
 *  card chrome: hairline dividers part the panes and the reading pane
 *  steps up to its paper. The folder pane is the shell's left column, not
 *  a panel here. The composer lives in the reading pane's quick-reply
 *  strip and in pop-out windows now, so the row holds no third column.
 *  The agent dock joins the Group key, so the layout remounts cleanly
 *  when it arrives or leaves. The default sizes sum to one hundred in
 *  each combination. */
function MailRow() {
  const agentOpen = useUiStore((s) => s.agentOpen);

  return (
    <Group
      key={agentOpen ? 'agent' : 'solo'}
      orientation="horizontal"
      className="h-full"
    >
      <Panel id="message-list" defaultSize={pct(agentOpen ? 38 : 47)} minSize={pct(25)}>
        <MessageList />
      </Panel>
      <VDivider />
      <Panel id="reading-pane" defaultSize={pct(agentOpen ? 34 : 53)} minSize={pct(20)}>
        <ReadingPane />
      </Panel>
      {agentOpen && <VDivider />}
      {agentOpen && (
        <Panel id="agent-dock" defaultSize={pct(28)} minSize={pct(14)} maxSize={pct(40)}>
          <AgentPanel />
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

  return (
    <div className="relative flex h-full flex-col overflow-hidden bg-background font-ui text-foreground">
      <TitleBar />
      {/* The middle row carries the folder pane's tone, so the right
          zone's carved corners fill with the left column's colour. */}
      <div className={`flex min-h-0 flex-1 ${shellView === 'mail' ? 'bg-surface' : ''}`}>
        <ToolWindowBar />
        {shellView === 'mail' && !compact && (
          /* The folder pane stands flush in the column (no padding around
             it), bordered, its left corners rounding against the rail's
             chrome the way the right zone rounds against the column. */
          <div className="w-[var(--folders-w)] shrink-0 bg-chrome">
            <div className="h-full overflow-hidden rounded-l-xl border border-border-subtle bg-surface">
              <FolderPane />
            </div>
          </div>
        )}
        {/* The right zone rounds its two left corners into the column: the
            junctions at the title bar and the status bar read as one carved
            surface, not two abutting frames. */}
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden rounded-l-xl">
          <CommandRibbon />
          {/* The mail view runs flush; the card surface stays for the
              calendar and people views. */}
          <main
            className={
              shellView === 'mail'
                ? 'min-h-0 min-w-0 flex-1'
                : 'min-h-0 min-w-0 flex-1 bg-surface p-2'
            }
          >
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
                <MailRow />
              )}
              {shellView === 'mail' && compact && (
                <FolderPaneDrawer>
                  <FolderPane />
                </FolderPaneDrawer>
              )}
            </div>
          </main>
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
