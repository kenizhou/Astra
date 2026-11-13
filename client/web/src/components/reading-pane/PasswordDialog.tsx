// SPDX-License-Identifier: GPL-3.0-only
// The reading pane's password dialog (SP4c-3c): the prompt the needs-password
// verdict mounts. A passphrase-protected message (a SKESK slot no key fits)
// opens fail closed with the banner alone; this dialog collects the reader's
// passphrase and submits it in-process to the host, which re-runs the password
// orchestrator and refreshes the reading snapshot. On a wrong passphrase the
// dialog re-prompts, naming the refusal; on a decrypt the refreshed snapshot
// drops the affordance and the dialog closes over the words. A cancel closes
// the dialog locally and leaves the verdict needs-password (fail closed): the
// passphrase is never logged or persisted, and a cancel never submits one.

import { useState, type FormEvent } from 'react';

import { useDialogFocus } from '@/components/ui/useDialogFocus';
import { submitReadingPassword, type PasswordPromptState } from '@/services/crypto';
import type { MessageKey } from '@/services/ipc';
import { useReadingStore } from '@/stores/readingStore';

/** The store gate: the dialog stands while the open message carries a password
 *  prompt, keyed by the message so every open starts empty with its own focus
 *  move. A decrypt drops the affordance and unmounts it. */
export function PasswordDialog() {
  const snapshot = useReadingStore((s) => s.snapshot);
  if (snapshot === null || snapshot.password_prompt === undefined) {
    return null;
  }
  const message = snapshot.message;
  return (
    <PasswordDialogBody
      key={`${message.account}:${message.key}`}
      message={message}
      prompt={snapshot.password_prompt}
    />
  );
}

/** The modal over one passphrase-protected message: the input, the re-prompt
 *  refusal, and the unlock and cancel verbs. */
function PasswordDialogBody({
  message,
  prompt,
}: {
  message: MessageKey;
  prompt: PasswordPromptState;
}) {
  const [passphrase, setPassphrase] = useState('');
  const [wrong, setWrong] = useState(prompt === 'wrong_password');
  const [busy, setBusy] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const inputRef = useDialogFocus<HTMLInputElement>();

  if (dismissed) {
    return null;
  }

  const cancel = (): void => setDismissed(true);

  const submit = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    if (passphrase === '' || busy) {
      return;
    }
    setBusy(true);
    try {
      const result = await submitReadingPassword(message, passphrase);
      if (result === 'wrong_password') {
        // The passphrase fit no slot: re-prompt, naming the refusal, and hand
        // the focus back to the empty input. No plaintext ever rendered.
        setWrong(true);
        setPassphrase('');
        inputRef.current?.focus();
      } else {
        // Decrypted (or a structural failure): the refreshed snapshot drops the
        // affordance and unmounts the dialog; close locally too so it never
        // lingers over the words while the snapshot pull lands.
        setDismissed(true);
      }
    } finally {
      setBusy(false);
    }
  };

  const field =
    'w-full rounded-[var(--radius-xs)] border border-border-subtle bg-background px-2.5 py-1.5 text-sm text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';
  const footer =
    'rounded-[var(--radius)] px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]';

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          cancel();
        }
      }}
    >
      <form
        role="dialog"
        aria-modal="true"
        aria-label="Message password"
        onSubmit={(event) => void submit(event)}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            cancel();
          }
        }}
        className="flex w-[380px] flex-col rounded-xl border border-border-subtle bg-card p-5 shadow-[var(--shadow-lg)]"
      >
        <h2 className="mb-2 text-sm font-semibold text-foreground">Password required</h2>
        <p className="text-xs text-muted-foreground">
          This message is encrypted with a passphrase. Enter it to open the message.
        </p>

        <label className="mt-3 flex flex-col gap-1 text-xs text-muted-foreground">
          Passphrase
          <input
            ref={inputRef}
            type="password"
            autoComplete="off"
            value={passphrase}
            disabled={busy}
            onChange={(event) => {
              setPassphrase(event.target.value);
              setWrong(false);
            }}
            className={field}
          />
        </label>

        {wrong && (
          <p role="alert" className="mt-2 text-xs text-destructive">
            The passphrase was not correct. Try again.
          </p>
        )}

        <div className="mt-4 flex justify-end gap-2 border-t border-border-subtle pt-3">
          <button
            type="button"
            onClick={cancel}
            disabled={busy}
            className={`${footer} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={passphrase === '' || busy}
            className={`${footer} bg-primary font-medium text-primary-foreground shadow-[var(--shadow-sm)] hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50`}
          >
            {busy ? 'Opening…' : 'Open message'}
          </button>
        </div>
      </form>
    </div>
  );
}