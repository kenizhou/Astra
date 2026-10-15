// SPDX-License-Identifier: GPL-3.0-only
// The AI documents panel: the digest and report tabs over their latest
// stored documents, the days selector beside the Generate and Regenerate
// button, and the alert region that surfaces a refused read or
// generation above whatever document still shows. The panel rides above
// the status bar's right corner beside the agent and outbox panels,
// opened by the tool rail's AI toggle; it serves the selected folder's
// account, the first account until one is.

import { useEffect } from 'react';

import { DAYS_MAX, DAYS_MIN, useAiPanelStore } from './aiPanelStore';
import { AiIcon, CloseIcon } from '@/components/icons';
import { RAIL_BUTTON_CLASS, railButtonColours } from '@/components/layout/railButton';
import type { DocumentKind } from '@/services/ai';
import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useUiStore } from '@/stores/uiStore';

const TABS: ReadonlyArray<{ id: DocumentKind; label: string }> = [
  { id: 'digest', label: 'Digest' },
  { id: 'report', label: 'Report' },
];

/** The month abbreviations the period label uses, in UTC order. */
const MONTHS = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
] as const;

/** One period bound as the label writes it: day, month, and year. */
function dayWord(stamp: number): string {
  const day = new Date(stamp * 1000);
  return `${day.getUTCDate()} ${MONTHS[day.getUTCMonth()]} ${day.getUTCFullYear()}`;
}

/** The stored window as the label shows it: whole UTC days, start to end. */
function periodLabel(start: number, end: number): string {
  return `${dayWord(start)} to ${dayWord(end)}`;
}

/** One kind's tab button. */
function TabButton({ id, label }: { id: DocumentKind; label: string }) {
  const tab = useAiPanelStore((s) => s.tab);
  return (
    <button
      type="button"
      role="tab"
      aria-selected={tab === id}
      onClick={() => useAiPanelStore.getState().setTab(id)}
      className={`rounded-[var(--radius-xs)] px-2 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
        tab === id
          ? 'bg-selected font-medium text-foreground'
          : 'text-muted-foreground hover:bg-hover hover:text-foreground'
      }`}
    >
      {label}
    </button>
  );
}

/** The AI panel over the shell's corner; renders nothing when shut. */
export function AiPanel() {
  const open = useUiStore((s) => s.aiPanelOpen);
  const accounts = useAccountStore((s) => s.accounts);
  const selectedFolder = useFolderStore((s) => s.selected);
  const account = useAiPanelStore((s) => s.account);
  const tab = useAiPanelStore((s) => s.tab);
  const days = useAiPanelStore((s) => s.days);
  const slot = useAiPanelStore((s) => s.docs[s.tab]);
  const generating = useAiPanelStore((s) => s.generating);

  const accountChoice = selectedFolder?.account ?? accounts[0]?.accountId ?? null;

  useEffect(() => {
    if (open && accountChoice !== null) {
      useAiPanelStore.getState().open(accountChoice);
    }
  }, [open, accountChoice]);

  if (!open) {
    return null;
  }

  const kindWord = tab === 'digest' ? 'digest' : 'report';
  const generateLabel =
    generating !== null ? 'Generating' : slot.document === null ? 'Generate' : 'Regenerate';

  return (
    <section
      aria-label="AI documents"
      className="absolute bottom-[calc(var(--status-h)+12px)] right-3 z-[var(--z-popover)] flex max-h-[70vh] w-[420px] flex-col overflow-hidden rounded-[var(--radius)] border border-border bg-chrome shadow-lg"
    >
      <div className="flex h-[var(--pane-header-h)] shrink-0 items-center justify-between border-b border-border px-3">
        <span className="text-xs font-medium text-foreground">AI documents</span>
        <button
          type="button"
          aria-label="Close AI documents"
          onClick={() => useUiStore.getState().setAiPanelOpen(false)}
          className="flex h-7 w-7 items-center justify-center rounded-md text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <CloseIcon size={14} />
        </button>
      </div>
      <div role="tablist" aria-label="AI documents" className="flex shrink-0 items-center gap-1 border-b border-border px-2 py-1.5">
        {TABS.map(({ id, label }) => (
          <TabButton key={id} id={id} label={label} />
        ))}
      </div>
      {account === null ? (
        <p className="px-3 py-3 text-xs text-muted-foreground">No account to read.</p>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
          {slot.error !== null && (
            <p role="alert" className="mb-2 text-[11px] text-destructive">
              {slot.error.message}
            </p>
          )}
          {slot.document === null && slot.loading ? (
            <p aria-live="polite" className="text-[11px] text-muted-foreground">
              Loading
            </p>
          ) : slot.document === null ? (
            <p className="text-[11px] text-muted-foreground">No {kindWord} yet.</p>
          ) : (
            <>
              <p className="text-[11px] font-medium text-foreground">
                {periodLabel(slot.document.period_start, slot.document.period_end)}
              </p>
              <div className="mt-1 whitespace-pre-wrap text-xs text-foreground">
                {slot.document.content}
              </div>
            </>
          )}
        </div>
      )}
      <form
        className="flex shrink-0 items-center justify-between gap-2 border-t border-border px-2 py-2"
        onSubmit={(event) => {
          event.preventDefault();
          void useAiPanelStore.getState().generate(tab);
        }}
      >
        <div className="flex min-w-0 items-center gap-1">
          {accounts.length > 1 && (
            <select
              aria-label="Document account"
              value={account ?? undefined}
              onChange={(event) => useAiPanelStore.getState().open(event.target.value)}
              className="max-w-[140px] rounded-full border border-border bg-surface px-2.5 py-1 text-[11px] text-foreground transition-colors focus-visible:outline-none focus-visible:border-primary"
            >
              {accounts.map((row) => (
                <option key={row.accountId} value={row.accountId}>
                  {row.displayName}
                </option>
              ))}
            </select>
          )}
          <select
            aria-label="Window days"
            value={days}
            onChange={(event) => useAiPanelStore.getState().setDays(Number(event.target.value))}
            className="rounded-full border border-border bg-surface px-2.5 py-1 text-[11px] text-foreground transition-colors focus-visible:outline-none focus-visible:border-primary"
          >
            {Array.from({ length: DAYS_MAX - DAYS_MIN + 1 }, (_, index) => DAYS_MIN + index).map(
              (value) => (
                <option key={value} value={value}>
                  {value === 1 ? '1 day' : `${value} days`}
                </option>
              ),
            )}
          </select>
        </div>
        <button
          type="submit"
          disabled={generating !== null || account === null}
          className="rounded-[var(--radius-xs)] bg-selected px-2 py-1 text-[11px] font-medium text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-60"
        >
          {generateLabel}
        </button>
      </form>
    </section>
  );
}

/** The tool rail's AI documents toggle. */
export function AiStatus() {
  const open = useUiStore((s) => s.aiPanelOpen);

  return (
    <button
      type="button"
      aria-label="AI"
      aria-pressed={open}
      title="AI documents"
      onClick={() => useUiStore.getState().setAiPanelOpen(!open)}
      className={`${RAIL_BUTTON_CLASS} ${railButtonColours(open)}`}
    >
      <AiIcon size={22} />
    </button>
  );
}
