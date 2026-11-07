// SPDX-License-Identifier: GPL-3.0-only
// The wizard's ask-once name step, the one step that follows the join:
// after an account connects, where no name stands for it yet (the
// provider's identities answered none, or the transport owns no name at
// all) the wizard asks what name its mail should go out under, once. The
// adoption rides the spawned sync, so a provider-known name can arrive
// mid-step and spend the ask under the person's typing: the step reads
// the CURRENT ask state when it renders and again when it settles, and a
// late adoption closes the step cleanly rather than writing a typed name
// over the adopted one. The empty name is a real answer, the deliberate
// bare address.

import { useEffect, useRef, useState } from 'react';

import { dispatch, intents, settings } from '@/services/ipc';
import { useSettingsStore } from '@/features/settings/settingsStore';
import { useDialogFocus } from '@/components/ui/useDialogFocus';

export interface SenderNameStepProps {
  /** The account whose name is asked, when the add path carried it. */
  account?: string;
  /** The address the account joined with: how the row is found when the
   *  add path carried no id (the OAuth completions answer it alone). */
  address: string;
  /** Closes the wizard and the step with it. */
  onDone: () => void;
}

/** The ask-once step: one name field, one continue, and the bare address
 *  as an honest alternative. */
export function SenderNameStep({ account, address, onDone }: SenderNameStepProps) {
  const [name, setName] = useState('');
  // The CURRENT ask state as the last pull answered it: the step never
  // asks on the strength of a snapshot older than the join.
  const [asking, setAsking] = useState<boolean | null>(null);
  const settled = useRef(false);
  const dialogRef = useDialogFocus<HTMLDivElement>(true);

  // The fresh read when the step renders: the join has landed by the time
  // the step stands (the completion joins before it answers), so a pull
  // names the account's row and its CURRENT ask state. An ask already
  // spent (the join adopted a name) closes the wizard without asking;
  // a pull that cannot answer closes the step rather than blocking the
  // wizard behind it.
  useEffect(() => {
    let live = true;
    settings()
      .then((snapshot) => {
        if (!live) {
          return;
        }
        const row = (snapshot.accounts ?? []).find((candidate) =>
          account !== undefined ? candidate.account === account : candidate.address === address,
        );
        if (row === undefined || row.needs_sender_name !== true) {
          onDone();
          setAsking(false);
          return;
        }
        setAsking(true);
      })
      .catch(() => {
        if (live) {
          onDone();
        }
      });
    return () => {
      live = false;
    };
    // The step mounts once per added account; the ids seed it.
  }, [account, address, onDone]);

  // The settings store's row for the account, as the republish carries
  // it: a late adoption turns the ask off mid-step, and the step closes
  // on it cleanly, never writing over the adopted name.
  const storedRow = useSettingsStore((s) =>
    (s.accounts ?? []).find((candidate) =>
      account !== undefined ? candidate.account === account : candidate.address === address,
    ),
  );
  useEffect(() => {
    if (storedRow !== undefined && !storedRow.needs_sender_name) {
      onDone();
    }
  }, [storedRow, onDone]);

  if (asking === false) {
    // The adoption already spent the ask: the wizard closes without
    // asking anything.
    return null;
  }
  if (asking === null) {
    // The pull has not answered; nothing draws.
    return null;
  }

  // The ask settles against the CURRENT state: a click that lands after
  // an adoption spends nothing.
  const settle = (value: string): void => {
    if (settled.current) {
      return;
    }
    const current = useSettingsStore.getState().accounts.find((candidate) =>
      account !== undefined ? candidate.account === account : candidate.address === address,
    );
    if (current !== undefined && !current.needs_sender_name) {
      onDone();
      return;
    }
    settled.current = true;
    const target = current?.account ?? account;
    if (target !== undefined) {
      void dispatch(intents.setSenderName(target, value));
    }
    onDone();
  };

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)]"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onDone();
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="sender-name-title"
        ref={dialogRef}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            onDone();
          }
        }}
        className="w-[26rem] rounded-[var(--radius)] border border-border bg-card p-5 shadow-[var(--shadow-lg)] focus-visible:outline-none"
      >
        <h2 id="sender-name-title" className="mb-1 text-[15px] font-semibold text-card-foreground">
          The account is connected
        </h2>
        <p className="mb-4 text-xs text-muted-foreground">
          One question and the mail from {address} carries the answer on its From line.
        </p>
        <form
          className="grid gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            settle(name);
          }}
        >
          <label className="grid gap-1 text-xs text-muted-foreground">
            What name should the mail go out under?
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
              autoComplete="off"
              spellCheck={false}
              placeholder="Ada Lovelace"
              className="rounded-full border border-input bg-background px-3 py-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-primary"
            />
          </label>
          <p className="-mt-2 text-[11px] text-muted-foreground">
            Leave it empty and the mail goes out as the address alone.
          </p>
          <div className="mt-1 flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={() => settle('')}
              className="rounded-[var(--radius-xs)] px-3 py-1.5 text-[13px] text-muted-foreground hover:bg-hover hover:text-foreground focus-visible:outline-none transition-colors focus-visible:border-primary"
            >
              Use the address alone
            </button>
            <button
              type="submit"
              className="rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 focus-visible:outline-none transition-colors focus-visible:border-primary"
            >
              Continue
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
