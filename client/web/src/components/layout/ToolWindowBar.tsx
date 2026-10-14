// SPDX-License-Identifier: GPL-3.0-only
// The tool rail: the shell's view switch as an icon column down the window's
// left edge. The view buttons pin the active view with a primary bar; the
// foot cluster holds the folder drawer toggle (compact windows only) and the
// agent and AI documents toggles.

import { AgentStatus } from '@/features/agent/AgentPanel';
import { AiStatus } from '@/features/ai-panel/AiPanel';
import { CalendarIcon, MailIcon, PanelLeftIcon, PeopleIcon } from '@/components/icons';
import { useIsCompact } from '@/hooks/useWindowSize';
import { useUiStore } from '@/stores/uiStore';
import type { IconProps } from '@/components/icons';
import type { ShellView } from '@/stores/uiStore';
import { RAIL_BUTTON_CLASS, railButtonColours } from './railButton';

/** The shell's views the rail offers, in the shell's own order. */
const VIEWS: ReadonlyArray<{
  id: ShellView;
  label: string;
  Icon: (props: IconProps) => React.JSX.Element;
}> = [
  { id: 'mail', label: 'Mail', Icon: MailIcon },
  { id: 'calendar', label: 'Calendar', Icon: CalendarIcon },
  { id: 'people', label: 'People', Icon: PeopleIcon },
];

export function ToolWindowBar() {
  const shellView = useUiStore((s) => s.shellView);
  const setShellView = useUiStore((s) => s.setShellView);
  const drawerOpen = useUiStore((s) => s.folderDrawerOpen);
  const compact = useIsCompact();

  return (
    <nav
      aria-label="Tool windows"
      className="flex w-[var(--tool-w)] shrink-0 flex-col items-center justify-between bg-chrome py-2"
    >
      <div className="flex flex-col items-center gap-2">
        {VIEWS.map(({ id, label, Icon }) => {
          const selected = shellView === id;
          return (
            <button
              key={id}
              type="button"
              aria-label={label}
              aria-pressed={selected}
              title={label}
              onClick={() => setShellView(id)}
              className={`${RAIL_BUTTON_CLASS} ${railButtonColours(selected)}`}
            >
              {selected && (
                <span
                  aria-hidden="true"
                  className="iris-line-v absolute bottom-2 left-0 top-2 w-[3px] rounded-r-full"
                />
              )}
              <Icon size={22} />
            </button>
          );
        })}
      </div>

      <div className="flex flex-col items-center gap-2">
        <button
          type="button"
          aria-label="Toggle folder pane"
          aria-pressed={drawerOpen}
          title="Toggle folder pane"
          onClick={() => useUiStore.getState().setFolderDrawerOpen(!drawerOpen)}
          className={`${RAIL_BUTTON_CLASS} ${railButtonColours(drawerOpen)} ${compact ? '' : 'hidden'}`}
        >
          <PanelLeftIcon size={22} />
        </button>
        <AgentStatus />
        <AiStatus />
      </div>
    </nav>
  );
}
