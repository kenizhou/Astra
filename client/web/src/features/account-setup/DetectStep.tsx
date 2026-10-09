// SPDX-License-Identifier: GPL-3.0-only
// The setup wizard's first step: one email address, whose server settings the
// host looks up while the step shows it is busy, with the manual form one
// click away for whoever would rather not wait or share the address.

import { useState, type FormEvent } from 'react';

interface DetectStepProps {
  busy: boolean;
  onCancel: () => void;
  onDetect: (email: string) => void;
  onManual: (email: string) => void;
}

const INPUT_CLASS =
  'rounded-[var(--radius-xs)] border border-input bg-background px-2 py-1.5 text-[13px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring';

export function DetectStep({ busy, onCancel, onDetect, onManual }: DetectStepProps) {
  const [email, setEmail] = useState('');

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!busy) {
      onDetect(email.trim());
    }
  };

  return (
    <form onSubmit={submit} className="grid gap-3">
      <p className="text-xs text-muted-foreground">
        Enter the email address and its server settings are looked up.
      </p>
      <label className="grid gap-1 text-xs text-muted-foreground">
        Email address
        <input
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          autoComplete="off"
          spellCheck={false}
          placeholder="name@example.net"
          disabled={busy}
          className={INPUT_CLASS}
        />
      </label>

      <div className="mt-1 flex items-center justify-between gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="rounded-[var(--radius-xs)] px-3 py-1.5 text-[13px] text-muted-foreground hover:bg-hover hover:text-foreground"
        >
          Cancel
        </button>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => onManual(email.trim())}
            disabled={busy}
            className="rounded-[var(--radius-xs)] bg-secondary px-3 py-1.5 text-[13px] text-secondary-foreground hover:bg-hover disabled:opacity-40"
          >
            Set up manually
          </button>
          <button
            type="submit"
            disabled={busy}
            className="rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-40"
          >
            {busy ? 'Looking up the settings…' : 'Continue'}
          </button>
        </div>
      </div>
    </form>
  );
}
