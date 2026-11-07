// SPDX-License-Identifier: GPL-3.0-only
// The account's outgoing name card on the accounts page: the one control
// family the sender-name governance answers. Where the provider holds the
// name and the account holder may change it, the card edits and pushes
// best-effort, the refusal sentence standing under a push the server
// refused while the local name stands. Where the provider holds it and
// the account holder may not, the card shows it with its reason and
// offers no field. Where the transport owns no name at all, the card
// edits the app's own stored value and says where it lives. An account
// whose name is unanswered asks once here, the same ask the wizard draws
// at connect: the headless add's route to the step.

import { useState } from 'react';

import { dispatch, intents } from '@/services/ipc';
import type { AccountSettingsRow } from '@/services/ipc';

/** The note each governance state carries under the field or the name. */
const CONTROL_NOTES: Record<NonNullable<AccountSettingsRow['sender_name_control']>, string> = {
  writable: 'The name is pushed to the server too; a push the server refuses leaves the name here.',
  read_only: 'The provider holds this name and it cannot be changed here.',
  local: 'This name is kept in Astra Mail; the server has no name of its own.',
};

export interface SenderNameCardProps {
  /** The account row whose name the card carries. */
  row: AccountSettingsRow;
}

/** The outgoing-name card: the ask once, then the three controls. */
export function SenderNameCard({ row }: SenderNameCardProps) {
  // The field seeds from the row and RESEEDS when the ask state turns, so
  // a late adoption (the join's identity read landing while the page
  // stands) cannot leave an empty field over an adopted name: one
  // unthinking Save would clobber the adopted name to the bare address.
  const asked = row.needs_sender_name === true;
  const [askSeen, setAskSeen] = useState(asked);
  const [name, setName] = useState(() =>
    row.display_name === row.address ? '' : (row.display_name ?? ''),
  );
  if (askSeen && !asked) {
    setAskSeen(false);
    setName(row.display_name === row.address ? '' : (row.display_name ?? ''));
  }
  const control = row.sender_name_control ?? 'local';

  // The ask-once state: no answer stands, not even the deliberate empty
  // one. The ask leaves with the snapshot that carries the answer.
  if (asked) {
    return (
      <div
        role="group"
        aria-label={`Outgoing name for ${row.address}`}
        className="flex max-w-[320px] flex-col gap-1.5"
      >
        <p className="text-xs font-medium text-foreground">Outgoing name</p>
        <p className="text-xs text-muted-foreground">
          What name should the mail from {row.address} go out under? Leave it empty and the mail
          goes out as the address alone.
        </p>
        <div className="flex items-center gap-2">
          <input
            aria-label={`What name ${row.address} goes out under`}
            value={name}
            onChange={(event) => setName(event.target.value)}
            autoComplete="off"
            spellCheck={false}
            className="min-w-0 flex-1 rounded-[var(--radius-xs)] border border-border bg-background px-2 py-1 text-xs text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
          <button
            type="button"
            onClick={() => void dispatch(intents.setSenderName(row.account, name))}
            className="shrink-0 rounded-[var(--radius)] bg-secondary px-2.5 py-1 text-xs text-secondary-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
          >
            Set the name
          </button>
          <button
            type="button"
            onClick={() => void dispatch(intents.setSenderName(row.account, ''))}
            className="shrink-0 rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
          >
            Use the address alone
          </button>
        </div>
      </div>
    );
  }

  const error = row.sender_name_error;

  return (
    <div
      role="group"
      aria-label={`Outgoing name for ${row.address}`}
      className="flex max-w-[320px] flex-col gap-1.5"
    >
      <p className="text-xs font-medium text-foreground">Outgoing name</p>
      {control === 'read_only' ? (
        <>
          <p className="text-xs text-foreground">{row.display_name}</p>
          <p className="text-xs text-muted-foreground">{CONTROL_NOTES[control]}</p>
        </>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <input
              aria-label={`The name on the From line for ${row.address}`}
              value={name}
              onChange={(event) => setName(event.target.value)}
              autoComplete="off"
              spellCheck={false}
              className="min-w-0 flex-1 rounded-[var(--radius-xs)] border border-border bg-background px-2 py-1 text-xs text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
            <button
              type="button"
              onClick={() => void dispatch(intents.setSenderName(row.account, name))}
              className="shrink-0 rounded-[var(--radius)] bg-secondary px-2.5 py-1 text-xs text-secondary-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]"
            >
              Save the name
            </button>
          </div>
          <p className="text-xs text-muted-foreground">{CONTROL_NOTES[control]}</p>
        </>
      )}
      {error !== undefined && (
        <p role="alert" className="text-xs text-[var(--warning)]">
          {error}
        </p>
      )}
    </div>
  );
}
