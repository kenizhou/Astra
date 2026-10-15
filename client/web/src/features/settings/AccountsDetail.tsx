// SPDX-License-Identifier: GPL-3.0-only
// The Accounts detail: one row per configured account (the address, the
// kind, the dialled legs), each row opening its own page: the address and
// what it is used for, the dav section editor, and Remove account behind a
// confirmation that names the address and the destructive act it takes.
// The removal's own failure sentence renders when it lands.

import { useState } from 'react';

import { dispatch, intents } from '@/services/ipc';
import type { AccountKind } from '@/services/ipc';

import { DavEditor } from './DavEditor';
import { useSettingsStore } from './settingsStore';

/** The kind words the rows show. */
const KIND_LABELS: Record<AccountKind, string> = {
  imap_smtp: 'IMAP + SMTP',
  jmap: 'JMAP',
  dav: 'DAV',
};

/** The legs as one sentence, or the honest none. */
function legsOf(legs: { calendar: boolean; contacts: boolean }): string {
  const standing = [
    ...(legs.calendar ? ['calendar'] : []),
    ...(legs.contacts ? ['contacts'] : []),
  ];
  if (standing.length === 0) {
    return 'No calendar or contacts legs yet.';
  }
  return `Used for ${standing.join(' and ')}`;
}

/** The Accounts page of the settings screen. */
export function AccountsDetail() {
  const accounts = useSettingsStore((s) => s.accounts);
  const error = useSettingsStore((s) => s.error);
  const [selected, setSelected] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);

  const row = accounts.find((candidate) => candidate.account === selected);
  if (row === undefined) {
    return (
      <section aria-label="Accounts" className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold text-foreground">Accounts</h2>
        {accounts.length === 0 && (
          <p className="text-xs text-muted-foreground">No account is configured.</p>
        )}
        <div className="flex flex-col gap-1">
          {accounts.map((account) => (
            <button
              key={account.account}
              type="button"
              aria-label={`Open ${account.address}`}
              onClick={() => {
                setRemoving(false);
                setSelected(account.account);
              }}
              className="flex flex-col items-start gap-0.5 rounded-[var(--radius-xs)] border border-border px-3 py-2 text-left transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <span className="text-xs font-medium text-foreground">{account.address}</span>
              <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                <span>{KIND_LABELS[account.kind]}</span>
                <span aria-hidden="true">·</span>
                <span>{legsOf(account.legs)}</span>
              </span>
            </button>
          ))}
        </div>
      </section>
    );
  }

  const confirmRemove = (): void => {
    void dispatch(intents.removeAccount(row.account));
    setRemoving(false);
  };

  return (
    <section aria-label={`Account ${row.address}`} className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <button
          type="button"
          aria-label="Back to accounts"
          onClick={() => {
            setRemoving(false);
            setSelected(null);
          }}
          className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          Accounts
        </button>
        <h2 className="text-sm font-semibold text-foreground">{row.address}</h2>
      </div>
      <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <span>{KIND_LABELS[row.kind]}</span>
        <span aria-hidden="true">·</span>
        <span>{legsOf(row.legs)}</span>
      </p>
      {row.refresh_error !== undefined && (
        <p role="status" className="max-w-[420px] text-xs text-[var(--warning)]">
          {row.refresh_error}
        </p>
      )}
      {/* Keyed by the account: the form seeds once per account, so moving
          between pages never carries one account's edits into another. */}
      <DavEditor
        key={row.account}
        account={row.account}
        onClose={() => {
          setRemoving(false);
          setSelected(null);
        }}
      />
      <div className="mt-2 flex flex-col gap-2 border-t border-border pt-3">
        {removing ? (
          <div
            role="alertdialog"
            aria-label={`Remove ${row.address}`}
            className="flex flex-col gap-2 rounded-[var(--radius-xs)] border border-[var(--destructive)]/40 p-3"
          >
            <p className="text-xs text-foreground">
              Remove {row.address}? Its mail, calendar, and contacts go together, and the
              stored configuration goes with them. This cannot be undone.
            </p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                aria-label="Cancel removal"
                onClick={() => setRemoving(false)}
                className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                Cancel
              </button>
              <button
                type="button"
                aria-label={`Remove ${row.address}`}
                onClick={confirmRemove}
                className="rounded-[var(--radius)] border border-[var(--destructive)] px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              >
                Remove
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setRemoving(true)}
            className="self-start rounded-[var(--radius)] border border-[var(--destructive)] px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Remove account
          </button>
        )}
        {error === 'remove_failed' && (
          <p role="alert" className="text-xs text-[var(--warning)]">
            The account could not be removed. Try again.
          </p>
        )}
      </div>
    </section>
  );
}
