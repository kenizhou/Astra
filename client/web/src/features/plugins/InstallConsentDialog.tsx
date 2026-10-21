// SPDX-License-Identifier: GPL-3.0-only
// The install consent dialog: the offer's identity, publisher fingerprint,
// and privacy policy, with every requested permission as a checkbox that
// starts unchecked (fail closed). Install carries exactly the checked set;
// Cancel installs nothing and the bundle never unpacked.

import { useEffect, useRef, useState } from 'react';

import type { InstallOffer } from '@/services/plugins';
import { pluginInstall } from '@/services/plugins';

/** The permission wire name in the dialog's own words. */
const permissionWords: Record<string, string> = {
  read_item: 'Read the message you are looking at',
  read_mailbox: 'Read and change mail in your whole mailbox',
  compose_edit: 'Change what you are writing',
  send: 'Gate or send your outgoing mail',
  admin: 'Manage keys and run automatically at send time',
  'ai.infer': "Call the app's models through the host",
};

export function InstallConsentDialog({
  offer,
  path,
  onDone,
}: {
  offer: InstallOffer;
  path: string;
  onDone: (installed: boolean) => void;
}) {
  const [checked, setChecked] = useState<ReadonlySet<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const dialogRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    dialogRef.current?.focus();
  }, []);

  const toggle = (permission: string): void => {
    const next = new Set(checked);
    if (next.has(permission)) {
      next.delete(permission);
    } else {
      next.add(permission);
    }
    setChecked(next);
  };

  const install = async (): Promise<void> => {
    setBusy(true);
    setError('');
    try {
      await pluginInstall(path, [...checked]);
      onDone(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4">
      <div
        ref={dialogRef}
        role="dialog"
        aria-label={`Install ${offer.name}`}
        aria-modal="true"
        tabIndex={-1}
        className="w-[420px] max-w-full rounded-xl border border-border-subtle bg-card p-5 shadow-lg outline-none"
      >
        <h2 className="text-sm font-semibold text-foreground">Install {offer.name}?</h2>
        <p className="mt-1 text-xs text-muted-foreground">
          {offer.id} · {offer.version}
          {offer.publisher_fingerprint !== '' && ` · publisher ${offer.publisher_fingerprint}`}
        </p>
        {offer.privacy_policy_url !== undefined && (
          <p className="mt-1 text-xs text-muted-foreground">
            Privacy policy: {offer.privacy_policy_url}
          </p>
        )}
        <fieldset className="mt-3 space-y-1.5">
          <legend className="text-xs font-medium text-foreground">It asks for</legend>
          {offer.permissions.length === 0 && (
            <p className="text-xs text-muted-foreground">No permissions.</p>
          )}
          {offer.permissions.map((permission) => (
            <label key={permission} className="flex items-center gap-2 text-xs text-foreground">
              <input
                type="checkbox"
                checked={checked.has(permission)}
                onChange={() => toggle(permission)}
              />
              <span>
                {permissionWords[permission] ?? permission}
                <span className="text-muted-foreground"> ({permission})</span>
              </span>
            </label>
          ))}
        </fieldset>
        {error !== '' && (
          <p role="alert" className="mt-2 text-[11px] text-destructive">
            {error}
          </p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={() => onDone(false)}
            className="rounded-[var(--radius-xs)] px-3 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void install()}
            className="rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
          >
            Install
          </button>
        </div>
      </div>
    </div>
  );
}
