// SPDX-License-Identifier: GPL-3.0-only
// The Reading detail: the conversation grouping, crossing through the
// settings family's own view-mode arm. The list toolbar still ships its
// sibling control through the delegating arm; both controls ask, and this
// one persists the answer through the settings family. The list itself
// still renders whatever the mailbox-list snapshot carries.

import { dispatch, intents } from '@/services/ipc';
import type { ViewMode } from '@/services/ipc';

import { PERSIST_SENTENCE } from './GeneralDetail';
import { useSettingsStore } from './settingsStore';

/** The Reading page of the settings screen. */
export function ReadingDetail() {
  const settings = useSettingsStore((s) => s.settings);
  const error = useSettingsStore((s) => s.error);

  return (
    <section aria-label="Reading settings" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">Reading</h2>
      <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
        Grouping
        <select
          aria-label="Grouping"
          value={settings.view_mode}
          onChange={(event) =>
            void dispatch(intents.setSettingsViewMode(event.target.value as ViewMode))
          }
          className="h-7 w-full rounded-[var(--radius-xs)] border border-input bg-background px-2 text-xs text-foreground outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring"
        >
          <option value="flat">One message per row</option>
          <option value="threaded">Group conversations</option>
        </select>
      </label>
      {error === 'persist_failed' && (
        <p role="alert" className="text-xs text-[var(--warning)]">
          {PERSIST_SENTENCE}
        </p>
      )}
    </section>
  );
}
