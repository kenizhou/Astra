// SPDX-License-Identifier: GPL-3.0-only
// The status bar: one clickable lamp per account saying where its sync
// stands (a click asks for a refresh), a send lamp while the outbox holds
// sends that have not gone out, and the folder the list is showing beside
// the selection count and the density cycle. The engine's own startup
// shows first; until the first pull lands nothing else can be true yet.
// The agent and AI documents toggles live on the tool rail.

import { DENSITIES, nextDensity } from '@/components/message-list/density';
import { dispatch, intents } from '@/services/ipc';
import { useAccountStore } from '@/stores/accountStore';
import { useFolderStore } from '@/stores/folderStore';
import { useListStore } from '@/stores/listStore';
import { useSendingStore } from '@/stores/sendingStore';
import { useUiStore } from '@/stores/uiStore';

export function StatusBar() {
  const engineState = useUiStore((s) => s.engineState);
  const sync = useUiStore((s) => s.sync);
  const accounts = useAccountStore((s) => s.accounts);
  const selected = useFolderStore((s) => s.selected);
  const outboxAccount = useFolderStore((s) => s.outboxAccount);
  // Sends still owed to the server: queued, or in flight right now.
  const pendingSends = useSendingStore(
    (s) => s.snapshot.rows.filter((row) => row.state === 'queued' || row.state === 'sending').length,
  );
  // The list's selection is a single row today; the wording is ready for
  // the multi-select slice.
  const selectedCount = useListStore((s) => (s.selectedKey === null ? 0 : 1));
  const density = useListStore((s) => s.density);

  const nameOf = (accountId: string): string =>
    accounts.find((account) => account.accountId === accountId)?.displayName ?? accountId;

  const densityLabel = DENSITIES.find((d) => d.id === density)?.label ?? density;
  const placeLabel =
    selected !== null ? selected.key : outboxAccount !== null ? 'Outbox' : 'No folder selected';

  return (
    <footer className="flex h-[var(--status-h)] shrink-0 select-none items-center justify-between border-t border-border-subtle bg-chrome px-3 text-xs text-muted-foreground">
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
            <button
              key={entry.account}
              type="button"
              title={entry.detail ?? 'Refresh mail'}
              onClick={() => void dispatch(intents.refreshMail())}
              className="inline-flex items-center gap-1.5 rounded-[var(--radius-xs)] px-1 transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <span
                aria-hidden="true"
                className="inline-block h-1.5 w-1.5 rounded-full"
                style={{ backgroundColor: entry.state === 'syncing' ? 'var(--primary)' : 'var(--success)' }}
              />
              {nameOf(entry.account)}
              {entry.state === 'syncing' ? ' syncing' : ' up to date'}
            </button>
          ))
        )}
        {pendingSends > 0 && (
          <span className="inline-flex items-center gap-1.5">
            <span aria-hidden="true" className="inline-block h-1.5 w-1.5 rounded-full bg-primary" />
            {pendingSends} sending
          </span>
        )}
      </div>
      <div className="flex min-w-0 items-center gap-2">
        {selectedCount > 0 && <span>{selectedCount} selected</span>}
        <button
          type="button"
          aria-label={`Density: ${densityLabel}`}
          title="Switch list density"
          onClick={() => useListStore.getState().setDensity(nextDensity(density))}
          className="rounded-[var(--radius-xs)] px-1 transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {densityLabel}
        </button>
        <span className="truncate">{placeLabel}</span>
      </div>
    </footer>
  );
}
