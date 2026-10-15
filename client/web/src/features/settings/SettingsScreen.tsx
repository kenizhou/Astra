// SPDX-License-Identifier: GPL-3.0-only
// The settings screen: a centred modal over the shell in the kylins
// preferences idiom, a vertical tab rail beside the detail panel and a
// footer under both. The categories render exactly what ships and nothing
// else, Escape and the Done button return to the shell wherever the focus
// stands (the gear that opened the screen keeps the focus itself), the
// dialog is caged (the focus moves in on open, the modal names itself),
// and the surface pull hydrates the store every time the screen opens, so
// the controls reflect the persisted truth from the first paint.

import { useEffect, useRef, useState } from 'react';

import {
  AboutIcon,
  AdvancedIcon,
  CalendarIcon,
  ComposeIcon,
  PeopleIcon,
  ReadingIcon,
  SettingsIcon,
  ShortcutsIcon,
} from '@/components/icons';
import { settings } from '@/services/ipc';
import { useUiStore } from '@/stores/uiStore';

import { AboutDetail } from './AboutDetail';
import { AccountsDetail } from './AccountsDetail';
import { AdvancedDetail } from './AdvancedDetail';
import { CalendarDetail } from './CalendarDetail';
import { ComposingDetail } from './ComposingDetail';
import { GeneralDetail } from './GeneralDetail';
import { ReadingDetail } from './ReadingDetail';
import { ShortcutsDetail } from './ShortcutsDetail';
import { useSettingsStore, type SettingsCategory } from './settingsStore';

export type { SettingsCategory } from './settingsStore';

/** The categories the screen offers, in the settings taxonomy's own order. */
const CATEGORIES: ReadonlyArray<{
  id: SettingsCategory;
  label: string;
  Icon: (props: { size?: number }) => React.JSX.Element;
}> = [
  { id: 'general', label: 'General', Icon: SettingsIcon },
  { id: 'calendar', label: 'Calendar', Icon: CalendarIcon },
  { id: 'reading', label: 'Reading', Icon: ReadingIcon },
  { id: 'composing', label: 'Composing', Icon: ComposeIcon },
  { id: 'shortcuts', label: 'Shortcuts', Icon: ShortcutsIcon },
  { id: 'accounts', label: 'Accounts', Icon: PeopleIcon },
  { id: 'advanced', label: 'Advanced', Icon: AdvancedIcon },
  { id: 'about', label: 'About', Icon: AboutIcon },
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
    // A caller may have asked for one category (the Help menu's About); the
    // ask is spent the moment the screen opens on it.
    const requested = useSettingsStore.getState().requestedCategory;
    if (requested !== null) {
      useSettingsStore.getState().requestCategory(null);
      setCategory(requested);
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
        className="flex h-[80vh] max-h-full w-[860px] max-w-full flex-col overflow-hidden rounded-xl border border-border-subtle bg-card shadow-lg outline-none"
      >
        <div className="flex min-h-0 flex-1">
          <nav
            aria-label="Settings categories"
            className="flex w-56 shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-border bg-chrome p-3"
          >
            {CATEGORIES.map(({ id, label, Icon }) => {
              const selected = category === id;
              return (
                <button
                  key={id}
                  type="button"
                  aria-pressed={selected}
                  onClick={() => setCategory(id)}
                  className={`relative flex items-center gap-3 rounded-lg px-3 py-2.5 text-left text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                    selected
                      ? 'bg-primary-muted text-primary'
                      : 'text-muted-foreground hover:bg-hover hover:text-foreground'
                  }`}
                >
                  {selected && (
                    <span className="absolute top-1.5 bottom-1.5 left-0 w-[3px] rounded-r-full iris-line-v" />
                  )}
                  <Icon size={18} />
                  <span>{label}</span>
                </button>
              );
            })}
          </nav>
          <div className="flex min-w-0 flex-1 flex-col">
            <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-end border-b border-border-subtle px-2">
              <button
                type="button"
                aria-label="Close settings"
                onClick={close}
                className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
              >
                Close
              </button>
            </div>
            <div aria-label="Settings detail" className="min-h-0 flex-1 overflow-y-auto p-4">
              {category === 'general' && <GeneralDetail />}
              {category === 'calendar' && <CalendarDetail />}
              {category === 'reading' && <ReadingDetail />}
              {category === 'composing' && <ComposingDetail />}
              {category === 'shortcuts' && <ShortcutsDetail />}
              {category === 'accounts' && <AccountsDetail />}
              {category === 'advanced' && <AdvancedDetail />}
              {category === 'about' && <AboutDetail />}
            </div>
          </div>
        </div>
        <footer className="flex shrink-0 items-center justify-between border-t border-border-subtle px-4 py-3">
          <span className="text-xs text-muted-foreground">Changes are applied automatically.</span>
          <button
            type="button"
            onClick={close}
            className="rounded-lg bg-primary px-5 py-2 text-sm font-medium text-primary-foreground shadow-[var(--shadow-sm)] transition-opacity hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Done
          </button>
        </footer>
      </div>
    </div>
  );
}
