// SPDX-License-Identifier: GPL-3.0-only
// The notification history panel: a popover off the title bar's bell listing
// the notified mail newest first. Each row opens its message through the
// existing openMessage intent and closes the popover; a per-entry dismiss
// and a Clear all empty the record. Escape and a pointer down outside close
// it (the bell trigger excepted, so its own toggle is not doubled), and the
// focus moves into the dialog on open and back to the bell on close.

import { useEffect } from 'react';

import { BellIcon } from '@/components/icons';
import { useDialogFocus } from '@/components/ui/useDialogFocus';
import { dispatch, intents } from '@/services/ipc';
import { useAccountStore, type AccountRow } from '@/stores/accountStore';
import {
  useNotificationHistoryStore,
  type NotificationHistoryEntry,
} from '@/stores/notificationHistoryStore';

interface NotificationHistoryPanelProps {
  /** Closes the popover; the bell's own state owns the open flag. */
  onClose: () => void;
}

/** The arrival clock label, UTC hours and minutes as the list stamps read. */
function formatTime(at: number): string {
  const stamp = new Date(at);
  const two = (value: number): string => String(value).padStart(2, '0');
  return `${two(stamp.getUTCHours())}:${two(stamp.getUTCMinutes())}`;
}

/** The account's display name, falling back to its id when unknown. */
function accountLabel(accounts: AccountRow[], account: string): string {
  return accounts.find((row) => row.accountId === account)?.displayName ?? account;
}

const ROW_BUTTON_CLASS =
  'min-w-0 flex-1 rounded-[var(--radius-sm)] px-2 py-1.5 text-left transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

const DISMISS_CLASS =
  'shrink-0 rounded px-1.5 py-0.5 text-[11px] text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

/** One recorded notification: the open button beside its dismiss action. */
function EntryRow({
  entry,
  accountName,
  onOpen,
  onDismiss,
}: {
  entry: NotificationHistoryEntry;
  accountName: string;
  onOpen: () => void;
  onDismiss: () => void;
}) {
  const label = entry.subject ?? entry.sender ?? entry.key;
  const hasDetail = entry.sender !== undefined || entry.subject !== undefined;
  return (
    <li
      data-testid={`notification-entry-${entry.id}`}
      className="flex items-start justify-between gap-1 px-1"
    >
      <button type="button" onClick={onOpen} aria-label={`Open: ${label}`} className={ROW_BUTTON_CLASS}>
        {hasDetail ? (
          <span className="block truncate text-xs">
            <span className="font-medium text-foreground">{entry.sender ?? 'Unknown sender'}</span>
            <span className="text-muted-text"> · {entry.subject ?? '(no subject)'}</span>
          </span>
        ) : (
          <span className="block truncate text-xs font-medium text-foreground">New message</span>
        )}
        <span className="mt-0.5 block text-[10px] text-muted-text">
          {formatTime(entry.at)} · {accountName}
        </span>
      </button>
      <button
        type="button"
        onClick={onDismiss}
        aria-label={`Dismiss: ${label}`}
        className={DISMISS_CLASS}
      >
        Dismiss
      </button>
    </li>
  );
}

/** The popover body, mounted only while the bell holds it open. */
export function NotificationHistoryPanel({ onClose }: NotificationHistoryPanelProps) {
  const entries = useNotificationHistoryStore((state) => state.entries);
  const accounts = useAccountStore((state) => state.accounts);
  const dismiss = useNotificationHistoryStore((state) => state.dismiss);
  const clear = useNotificationHistoryStore((state) => state.clear);
  const dialogRef = useDialogFocus<HTMLDivElement>();

  useEffect(() => {
    function onPointer(event: MouseEvent): void {
      const target = event.target as HTMLElement;
      if (dialogRef.current?.contains(target)) {
        return;
      }
      // The bell trigger toggles the popover itself; closing here too would
      // net its click a second toggle.
      if (target.closest('[data-notification-bell]') !== null) {
        return;
      }
      onClose();
    }
    document.addEventListener('mousedown', onPointer);
    return () => document.removeEventListener('mousedown', onPointer);
  }, [onClose, dialogRef]);

  const open = (entry: NotificationHistoryEntry): void => {
    void dispatch(intents.openMessage({ account: entry.account, key: entry.key }));
    onClose();
  };

  return (
    <div
      ref={dialogRef}
      role="dialog"
      aria-label="Notification history"
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.preventDefault();
          onClose();
        }
      }}
      className="flex w-[340px] max-w-[calc(100vw-24px)] flex-col rounded-[var(--radius-sm)] border border-border-subtle bg-surface-floating shadow-[var(--shadow-lg)] outline-none"
    >
      <div className="flex items-center justify-between border-b border-border-subtle px-3 py-2">
        <h2 className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
          <BellIcon size={14} />
          Notifications
        </h2>
        <button
          type="button"
          onClick={clear}
          disabled={entries.length === 0}
          className="rounded px-1.5 py-0.5 text-[11px] text-muted-text transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40"
        >
          Clear all
        </button>
      </div>

      <div className="max-h-[420px] overflow-y-auto py-1">
        {entries.length === 0 ? (
          <p className="px-3 py-6 text-center text-xs text-muted-text">No notifications yet.</p>
        ) : (
          <ul className="flex flex-col gap-0.5">
            {entries.map((entry) => (
              <EntryRow
                key={entry.id}
                entry={entry}
                accountName={accountLabel(accounts, entry.account)}
                onOpen={() => open(entry)}
                onDismiss={() => dismiss(entry.id)}
              />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}