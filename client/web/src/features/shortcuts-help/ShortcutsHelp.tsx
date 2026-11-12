// SPDX-License-Identifier: GPL-3.0-only
// The shortcuts help: a read-only reference overlay listing the shell's
// shortcuts grouped by where they answer, drawn from the settings inventory
// so the table stays the single source. Escape and the backdrop close, the
// focus moves into the dialog on open and back on close, and the surface
// mounts fresh on every open.

import { useDialogFocus } from '@/components/ui/useDialogFocus';
import { Kbd, SHORTCUTS, type ShortcutEntry } from '@/features/settings/ShortcutsDetail';

import { closeShortcutsHelp, useShortcutsHelpStore } from './useShortcutsHelp';

/** The inventory rows grouped by context, in first-appearance order. */
function groupByContext(entries: ReadonlyArray<ShortcutEntry>): Array<[string, ShortcutEntry[]]> {
  const groups = new Map<string, ShortcutEntry[]>();
  for (const entry of entries) {
    const rows = groups.get(entry.context);
    if (rows === undefined) {
      groups.set(entry.context, [entry]);
    } else {
      rows.push(entry);
    }
  }
  return [...groups.entries()];
}

/** The overlay, mounted in the shell's tail after the palette; null while closed. */
export function ShortcutsHelp() {
  const isOpen = useShortcutsHelpStore((state) => state.isOpen);
  if (!isOpen) {
    return null;
  }
  return <HelpSurface />;
}

/** The open surface: a read-only dialog over the grouped inventory. */
function HelpSurface() {
  const dialogRef = useDialogFocus<HTMLDivElement>();
  const groups = groupByContext(SHORTCUTS);

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-start justify-center bg-[var(--backdrop)] p-4 pt-[10vh]"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          closeShortcutsHelp();
        }
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.preventDefault();
            closeShortcutsHelp();
          }
        }}
        className="max-h-[80vh] w-full max-w-2xl overflow-y-auto rounded-xl border border-border-subtle bg-surface-floating shadow-[var(--shadow-lg)] outline-none"
      >
        <div className="border-b border-border-subtle px-5 py-3">
          <h2 className="text-base font-semibold text-foreground">Keyboard shortcuts</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            Press <Kbd>?</Kbd> to reopen. <Kbd>Esc</Kbd> closes.
          </p>
        </div>
        <div className="px-5 py-3">
          {groups.map(([context, entries]) => (
            <section key={context} aria-label={context} className="mb-4 last:mb-0">
              <h3 className="mb-1.5 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
                {context}
              </h3>
              <ul className="space-y-0.5">
                {entries.map((entry) => (
                  <li
                    key={`${entry.keys}:${entry.action}`}
                    className="flex items-center justify-between gap-4 rounded-md px-2 py-1 text-[13px] text-foreground"
                  >
                    <span className="min-w-0 flex-1">{entry.action}</span>
                    <span className="shrink-0">
                      <Kbd>{entry.keys}</Kbd>
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}