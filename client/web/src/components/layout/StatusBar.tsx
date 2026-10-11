// SPDX-License-Identifier: GPL-3.0-only
// The status bar: one line per account saying where its sync stands, beside
// the folder the list is showing and the outbox toggle with its outstanding
// count, the agent toggle, and the AI documents toggle. The engine's own
// startup shows first; until the first pull lands nothing else can be true
// yet.

import { AiStatus } from '@/features/ai-panel/AiPanel';
import { AgentStatus } from '@/features/agent/AgentPanel';
import { OutboxStatus } from '@/features/outbox/OutboxPanel';
import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useUiStore } from '@/stores/uiStore';

export function StatusBar() {
  const engineState = useUiStore((s) => s.engineState);
  const sync = useUiStore((s) => s.sync);
  const accounts = useAccountStore((s) => s.accounts);
  const selected = useFolderStore((s) => s.selected);

  const nameOf = (accountId: string): string =>
    accounts.find((account) => account.accountId === accountId)?.displayName ?? accountId;

  return (
    <footer className="flex h-[var(--status-h)] shrink-0 select-none items-center justify-between border-t border-border bg-chrome px-3 text-xs text-muted-foreground">
      <div className="flex min-w-0 items-center gap-4">
        {engineState === 'starting' ? (
          <span className="inline-flex items-center gap-1.5">
            <span aria-hidden="true" className="inline-block h-1.5 w-1.5 rounded-full bg-[var(--muted-text)]" />
            Starting
          </span>
        ) : sync.length === 0 ? (
          <span className="inline-flex items-center gap-1.5">
            <span aria-hidden="true" className="inline-block h-1.5 w-1.5 rounded-full bg-success" />
            No sync running
          </span>
        ) : (
          sync.map((entry) => (
            <span
              key={entry.account}
              title={entry.detail ?? undefined}
              className="inline-flex items-center gap-1.5"
            >
              <span
                aria-hidden="true"
                className="inline-block h-1.5 w-1.5 rounded-full"
                style={{ backgroundColor: entry.state === 'syncing' ? 'var(--primary)' : 'var(--success)' }}
              />
              {nameOf(entry.account)}
              {entry.state === 'syncing' ? ' syncing' : ' up to date'}
            </span>
          ))
        )}
      </div>
      <div className="flex min-w-0 items-center gap-2">
        <OutboxStatus />
        <AgentStatus />
        <AiStatus />
        <span className="truncate">{selected === null ? 'No folder selected' : selected.key}</span>
      </div>
    </footer>
  );
}
