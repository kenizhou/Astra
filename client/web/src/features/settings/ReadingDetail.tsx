// SPDX-License-Identifier: GPL-3.0-only
// The Reading detail: the conversation grouping, crossing through the
// settings family's own view-mode arm (the list toolbar still ships its
// sibling control through the delegating arm; both controls ask, and this
// one persists the answer through the settings family), and the swipe
// actions, one pick per direction, held device-local in the appearance
// store the way the reading pane's position is: a gesture preference of
// the device, not a mail preference that crosses accounts.

import { dispatch, intents } from '@/services/ipc';
import type { ViewMode } from '@/services/ipc';
import { SWIPE_ACTIONS } from '@/components/message-list/rowSwipe';
import { useAppearanceStore, type SwipeAction } from '@/services/appearance';

import { PERSIST_SENTENCE } from './GeneralDetail';
import { useSettingsStore } from './settingsStore';

const SELECT_CLASS =
  'h-7 w-full rounded-full border border-input bg-background px-3 text-xs text-foreground outline-none transition-colors focus:border-primary';

/** The Reading page of the settings screen. */
export function ReadingDetail() {
  const settings = useSettingsStore((s) => s.settings);
  const error = useSettingsStore((s) => s.error);
  const swipePrimary = useAppearanceStore((s) => s.swipePrimary);
  const swipeSecondary = useAppearanceStore((s) => s.swipeSecondary);
  const setSwipePrimary = useAppearanceStore((s) => s.setSwipePrimary);
  const setSwipeSecondary = useAppearanceStore((s) => s.setSwipeSecondary);

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
          className={SELECT_CLASS}
        >
          <option value="flat">One message per row</option>
          <option value="threaded">Group conversations</option>
        </select>
      </label>
      <div className="flex max-w-[280px] gap-3">
        <label className="flex flex-1 flex-col gap-1 text-xs text-muted-foreground">
          Swipe right
          <select
            aria-label="Swipe right"
            value={swipePrimary}
            onChange={(event) => setSwipePrimary(event.target.value as SwipeAction)}
            className={SELECT_CLASS}
          >
            {SWIPE_ACTIONS.map(({ id, label }) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-1 flex-col gap-1 text-xs text-muted-foreground">
          Swipe left
          <select
            aria-label="Swipe left"
            value={swipeSecondary}
            onChange={(event) => setSwipeSecondary(event.target.value as SwipeAction)}
            className={SELECT_CLASS}
          >
            {SWIPE_ACTIONS.map(({ id, label }) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </label>
      </div>
      {error === 'persist_failed' && (
        <p role="alert" className="text-xs text-[var(--warning)]">
          {PERSIST_SENTENCE}
        </p>
      )}
    </section>
  );
}
