// SPDX-License-Identifier: GPL-3.0-only
// The grant refusal surfaces: the reconnect banner for a Microsoft
// account whose calendar permission went missing (one tap re-runs the
// sign-in with the account's address as the login hint, the re-consent
// path the wizard's completion owns) and the expired-sign-in row for a
// dead grant, whose sentence names the sign-in and never a server
// outage. Both ride the accounts page over the standing grant state the
// settings snapshot's account rows carry; a healthy account renders
// neither.

import { oauthBegin } from '@/services/ipc';
import type { AccountSettingsRow } from '@/services/ipc';

/** The accounts the reconnect banner names: Microsoft accounts whose
 * grant withholds the calendar. The trigger is Microsoft-only: the
 * withheld calendar is Graph's own refusal, and the same word on another
 * kind renders nothing. */
export function withheldCalendarAccounts(
  accounts: AccountSettingsRow[],
): AccountSettingsRow[] {
  return accounts.filter(
    (row) => row.kind === 'microsoft' && row.grant === 'calendar_withheld',
  );
}

/** The banner over the accounts list: one sentence and one Reconnect per
 * withheld calendar. */
export function ReconnectBanner({ rows }: { rows: AccountSettingsRow[] }) {
  const withheld = withheldCalendarAccounts(rows);
  if (withheld.length === 0) {
    return null;
  }
  return (
    <div className="flex flex-col gap-2">
      {withheld.map((row) => (
        <div
          key={row.account}
          role="status"
          aria-label="Calendar permission missing"
          className="flex items-start justify-between gap-3 rounded-[var(--radius-xs)] border border-[var(--warning)]/40 p-3"
        >
          <p className="max-w-[420px] text-xs text-foreground">
            Microsoft reports the calendar permission missing for {row.address}. Mail keeps
            working; the calendar waits for a fresh sign-in.
          </p>
          <button
            type="button"
            aria-label={`Reconnect ${row.address}`}
            onClick={() => {
              void oauthBegin('microsoft', row.address);
            }}
            className="shrink-0 rounded-[var(--radius)] bg-secondary px-2.5 py-1 text-xs text-secondary-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
          >
            Reconnect
          </button>
        </div>
      ))}
    </div>
  );
}

/** The expired-sign-in row: a dead grant renders its own card, because a
 * sign-in button inside the open-button row is markup no screen reader
 * tolerates. The sentence names the sign-in, never an outage, and its
 * action is the same re-grant. */
export function ExpiredSignInRow({
  row,
  onOpen,
}: {
  row: AccountSettingsRow;
  onOpen: () => void;
}) {
  return (
    <div
      role="status"
      aria-label="Sign-in expired"
      className="flex flex-col items-start gap-1 rounded-[var(--radius-xs)] border border-[var(--warning)]/40 px-3 py-2"
    >
      <p className="text-xs font-medium text-foreground">{row.address}</p>
      <p className="text-xs text-muted-foreground">
        The sign-in expired or was withdrawn. Sign in again to reconnect this account.
      </p>
      <div className="flex gap-2 pt-1">
        <button
          type="button"
          aria-label={`Sign in again as ${row.address}`}
          onClick={() => {
            void oauthBegin('microsoft', row.address);
          }}
          className="rounded-[var(--radius)] bg-secondary px-2.5 py-1 text-xs text-secondary-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
        >
          Sign in again
        </button>
        <button
          type="button"
          aria-label={`Open ${row.address}`}
          onClick={onOpen}
          className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
        >
          Open
        </button>
      </div>
    </div>
  );
}
