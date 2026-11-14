// SPDX-License-Identifier: GPL-3.0-only
// The block-sender dialog: blocks one sender so their mail raises no new-mail
// notification, with an optional report leg that reuses the shipped spam
// report. Suppression only: the block never moves the mail, it still lands in
// the Inbox; filing as junk is the report's own act, offered here as a
// convenience and defaulted on where the account can report. The block is the
// durable act and stands whatever the report does: a report failure is logged,
// never rolled back. The row menu's Block entry opens it through the block
// store.

import { useState } from 'react';

import { useDialogFocus } from '@/components/ui/useDialogFocus';
import { dispatch, intents } from '@/services/ipc';

import { useBlockSenderStore, type BlockTarget } from './blockSenderStore';

/** The store gate: the dialog stands while a block ask is named, keyed by it
 *  so every open starts on the report default with its own focus move. */
export function BlockSenderDialog() {
  const target = useBlockSenderStore((s) => s.target);
  if (target === null) {
    return null;
  }
  return <BlockSenderDialogBody key={`${target.address}:${JSON.stringify(target.row)}`} target={target} />;
}

/** The modal over one sender: the block, and the optional report leg. */
function BlockSenderDialogBody({ target }: { target: BlockTarget }) {
  const [report, setReport] = useState(true);
  const dialogRef = useDialogFocus<HTMLDivElement>();

  const close = (): void => useBlockSenderStore.getState().close();
  const confirm = (): void => {
    // The block is the durable act; it lands whatever the report does.
    void dispatch(intents.blockSender(target.address));
    if (report && target.canReport) {
      // The report is best-effort: a failure is logged, never undoing the
      // block that already went out.
      void dispatch(intents.markJunk(target.row)).catch((err: unknown) => {
        console.error('junk blocking: the report could not be queued', err);
      });
    }
    close();
  };

  const footer =
    'rounded-[var(--radius)] px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]';

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          close();
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Block sender"
        ref={dialogRef}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            close();
          }
        }}
        className="flex max-h-full w-[420px] flex-col overflow-y-auto rounded-xl border border-border-subtle bg-card p-5 shadow-[var(--shadow-lg)] focus-visible:outline-none"
      >
        <h2 className="mb-3 text-sm font-semibold text-foreground">Block sender</h2>

        <p className="truncate text-xs text-muted-foreground" title={target.address}>
          {target.address}
        </p>
        <p className="mt-2 text-sm text-foreground">
          Their mail will no longer raise new-mail notifications.
        </p>
        <p className="mt-0.5 text-[11px] text-muted-text">
          It still arrives in your Inbox; blocking never moves it.
        </p>

        {target.canReport && (
          <label className="mt-3 flex items-center gap-2 text-xs text-foreground">
            <input
              type="checkbox"
              checked={report}
              onChange={(event) => setReport(event.target.checked)}
            />
            Also report this conversation as junk
          </label>
        )}

        <div className="mt-4 flex justify-end gap-2 border-t border-border-subtle pt-3">
          <button
            type="button"
            onClick={close}
            className={`${footer} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={confirm}
            className={`${footer} bg-primary font-medium text-primary-foreground shadow-[var(--shadow-sm)] hover:opacity-90`}
          >
            Block sender
          </button>
        </div>
      </div>
    </div>
  );
}