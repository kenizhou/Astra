// SPDX-License-Identifier: GPL-3.0-only
// The settings screen: the modal over the shell carrying the desktop
// pattern, a category source list beside the detail panel. The categories
// render exactly what ships and nothing else, Escape and the Close return
// to the shell wherever the focus stands (the gear that opened the screen
// keeps the focus itself), the dialog is caged (the focus moves in on
// open, the modal names itself), and the surface pull hydrates the store
// every time the screen opens, so the controls reflect the persisted
// truth from the first paint.

import { useEffect, useRef, useState } from 'react';

import { settings } from '@/services/ipc';
import { useUiStore } from '@/stores/uiStore';

import { AboutDetail } from './AboutDetail';
import { AccountsDetail } from './AccountsDetail';
import { AdvancedDetail } from './AdvancedDetail';
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
  | 'advanced'
  | 'about';

/** The categories the screen offers, in the settings taxonomy's own order. */
const CATEGORIES: ReadonlyArray<{ id: SettingsCategory; label: string }> = [
  { id: 'general', label: 'General' },
  { id: 'calendar', label: 'Calendar' },
  { id: 'reading', label: 'Reading' },
  { id: 'composing', label: 'Composing' },
  { id: 'accounts', label: 'Accounts' },
  { id: 'advanced', label: 'Advanced' },
  { id: 'about', label: 'About' },
];

/** The settings modal over the shell. */
export function SettingsScreen() {
  const open = useUiStore((s) => s.settingsOpen);
  const [category, setCategory] = useState<SettingsCategory>('general');
  const dialogRef = useRef<HTMLDivElement>(null);

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
    // The Escape answers at the document level: the gear that opened the
    // screen keeps the focus itself, so a listener inside the dialog alone
    // never sees the key. The focus moves into the caged dialog on open.
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        useUiStore.getState().setSettingsOpen(false);
      }
    };
    document.addEventListener('keydown', onKey);
    dialogRef.current?.focus();
    return () => {
      standing = false;
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  if (!open) {
    return null;
  }

  const close = (): void => useUiStore.getState().setSettingsOpen(false);

  return (
    <div className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4">
      <div
        ref={dialogRef}
        role="dialog"
        aria-label="Settings"
        aria-modal="true"
        tabIndex={-1}
        className="flex h-[560px] max-h-full w-[760px] max-w-full overflow-hidden rounded-[var(--radius)] border border-border bg-surface-floating shadow-[var(--shadow-lg)] outline-none"
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
            {category === 'advanced' && <AdvancedDetail />}
            {category === 'about' && <AboutDetail />}
          </div>
        </div>
      </div>
    </div>
  );
}
