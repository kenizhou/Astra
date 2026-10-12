// SPDX-License-Identifier: GPL-3.0-only
// The settings screen: the modal over the shell carrying the desktop
// pattern, a category source list beside the detail panel. The categories
// render exactly what ships and nothing else, Escape and the Close return
// to the shell, and the surface pull hydrates the store every time the
// screen opens, so the controls reflect the persisted truth from the
// first paint.

import { useEffect, useState } from 'react';

import { settings } from '@/services/ipc';
import { useUiStore } from '@/stores/uiStore';

import { AboutDetail } from './AboutDetail';
import { AccountsDetail } from './AccountsDetail';
import { CalendarDetail } from './CalendarDetail';
import { ComposingDetail } from './ComposingDetail';
import { GeneralDetail } from './GeneralDetail';
import { ReadingDetail } from './ReadingDetail';
import { useSettingsStore } from './settingsStore';

/** The category id, one per detail the screen ships. */
export type SettingsCategory =
  | 'general'
  | 'calendar'
  | 'reading'
  | 'composing'
  | 'accounts'
  | 'about';

/** The categories the screen offers, in the settings taxonomy's own order. */
const CATEGORIES: ReadonlyArray<{ id: SettingsCategory; label: string }> = [
  { id: 'general', label: 'General' },
  { id: 'calendar', label: 'Calendar' },
  { id: 'reading', label: 'Reading' },
  { id: 'composing', label: 'Composing' },
  { id: 'accounts', label: 'Accounts' },
  { id: 'about', label: 'About' },
];

/** The settings modal over the shell. */
export function SettingsScreen() {
  const open = useUiStore((s) => s.settingsOpen);
  const [category, setCategory] = useState<SettingsCategory>('general');

  useEffect(() => {
    if (!open) {
      return;
    }
    let standing = true;
    settings()
      .then((snapshot) => {
        if (standing) {
          useSettingsStore.getState().applySnapshot(snapshot);
        }
      })
      .catch((err: unknown) => {
        console.error('settings: the open pull failed', err);
      });
    return () => {
      standing = false;
    };
  }, [open]);

  if (!open) {
    return null;
  }

  const close = (): void => useUiStore.getState().setSettingsOpen(false);

  return (
    <div className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4">
      <div
        role="dialog"
        aria-label="Settings"
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            close();
          }
        }}
        className="flex h-[560px] max-h-full w-[760px] max-w-full overflow-hidden rounded-[var(--radius)] border border-border bg-surface-floating shadow-[var(--shadow-lg)]"
      >
        <nav
          aria-label="Settings categories"
          className="flex w-[180px] shrink-0 flex-col gap-0.5 border-r border-border bg-surface p-2"
        >
          {CATEGORIES.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              aria-pressed={category === id}
              onClick={() => setCategory(id)}
              className={`rounded-[var(--radius-xs)] px-2.5 py-1.5 text-left text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                category === id
                  ? 'bg-selected font-medium text-selected-text'
                  : 'text-muted-foreground hover:bg-hover hover:text-foreground'
              }`}
            >
              {label}
            </button>
          ))}
        </nav>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-end border-b border-border bg-surface px-2">
            <button
              type="button"
              aria-label="Close settings"
              onClick={close}
              className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Close
            </button>
          </div>
          <div aria-label="Settings detail" className="min-h-0 flex-1 overflow-y-auto p-4">
            {category === 'general' && <GeneralDetail />}
            {category === 'calendar' && <CalendarDetail />}
            {category === 'reading' && <ReadingDetail />}
            {category === 'composing' && <ComposingDetail />}
            {category === 'accounts' && <AccountsDetail />}
            {category === 'about' && <AboutDetail />}
          </div>
        </div>
      </div>
    </div>
  );
}
