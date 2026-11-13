// SPDX-License-Identifier: GPL-3.0-only
// The Keys detail: the key import affordance. A person imports their own
// private/identity key (so received encrypted mail decrypts) or a peer's
// public certificate (so signatures verify and encrypted sends resolve),
// by file or by paste, with an optional passphrase for a secret-bearing
// key. The account's imported keys list below. The passphrase is held only
// in memory (local state), cleared the moment it is spent, and never logged
// or persisted; the bytes cross into host memory where the host dispatches
// the public and private paths.

import { useCallback, useEffect, useState } from 'react';

import { importKey, listKeys, type CryptoKeyRow } from '@/services/crypto';
import { useSettingsStore } from '@/features/settings/settingsStore';
import { CollectedKeysReview } from './CollectedKeysReview';

const selectClass =
  'h-7 w-full rounded-full border border-input bg-background px-3 text-xs text-foreground outline-none transition-colors focus:border-primary';

/** The Keys page of the settings screen. */
export function KeysDetail() {
  const accounts = useSettingsStore((s) => s.accounts);
  const [account, setAccount] = useState(accounts[0]?.account ?? '');
  const [paste, setPaste] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [keys, setKeys] = useState<CryptoKeyRow[]>([]);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const chosen = accounts.find((row) => row.account === account)?.account ?? accounts[0]?.account ?? '';

  const refresh = useCallback(async (target: string): Promise<void> => {
    if (target === '') {
      setKeys([]);
      return;
    }
    try {
      setKeys(await listKeys(target));
    } catch (err) {
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    void refresh(chosen);
  }, [chosen, refresh]);

  // The one import seam: hand the bytes and the (optional) passphrase to the
  // host, then refresh the list. The passphrase is cleared the moment it is
  // spent, whether the import landed or refused: it lives only in memory.
  const runImport = async (bytes: Uint8Array): Promise<void> => {
    if (chosen === '' || bytes.length === 0) {
      return;
    }
    setBusy(true);
    setError(null);
    setNote(null);
    const secret = passphrase.length > 0 ? passphrase : undefined;
    try {
      const row = await importKey(chosen, bytes, secret);
      setNote(`Imported ${row.fingerprint}`);
      await refresh(chosen);
    } catch (err) {
      setError(String(err));
    } finally {
      setPassphrase('');
      setBusy(false);
    }
  };

  const onFile = async (file: File | undefined): Promise<void> => {
    if (file === undefined) {
      return;
    }
    const bytes = new Uint8Array(await file.arrayBuffer());
    await runImport(bytes);
  };

  const onPaste = async (): Promise<void> => {
    const bytes = new TextEncoder().encode(paste);
    await runImport(bytes);
    setPaste('');
  };

  return (
    <section aria-label="Key import settings" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">Keys</h2>
      <p className="max-w-[520px] text-xs text-muted-foreground">
        Import your own private key to read encrypted mail, or a peer&apos;s public
        certificate to verify their signatures and send them encrypted mail. A
        passphrase, when the key is protected, stays in memory only.
      </p>

      <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
        Account
        <select
          aria-label="Account"
          value={chosen}
          onChange={(event) => setAccount(event.target.value)}
          className={selectClass}
        >
          {accounts.map((row) => (
            <option key={row.account} value={row.account}>
              {row.address}
            </option>
          ))}
        </select>
      </label>

      <label className="flex max-w-[520px] flex-col gap-1 text-xs text-muted-foreground">
        Paste a key or certificate
        <textarea
          aria-label="Paste a key or certificate"
          value={paste}
          onChange={(event) => setPaste(event.target.value)}
          rows={5}
          placeholder="-----BEGIN PGP PUBLIC KEY BLOCK-----"
          className="w-full rounded-lg border border-input bg-background px-3 py-2 font-mono text-xs text-foreground outline-none transition-colors focus:border-primary"
        />
      </label>

      <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
        Passphrase (for a protected private key)
        <input
          type="password"
          aria-label="Passphrase"
          value={passphrase}
          onChange={(event) => setPassphrase(event.target.value)}
          autoComplete="off"
          className="h-7 w-full rounded-full border border-input bg-background px-3 text-xs text-foreground outline-none transition-colors focus:border-primary"
        />
      </label>

      <div className="flex items-center gap-2">
        <button
          type="button"
          disabled={busy || paste.length === 0}
          onClick={() => void onPaste()}
          className="rounded-lg bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          Import pasted key
        </button>
        <label className="cursor-pointer rounded-lg border border-border px-3 py-1.5 text-xs text-foreground transition-colors hover:bg-hover focus-within:ring-2 focus-within:ring-ring">
          Import a key file
          <input
            type="file"
            aria-label="Import a key file"
            className="sr-only"
            onChange={(event) => {
              void onFile(event.target.files?.[0]);
              event.target.value = '';
            }}
          />
        </label>
      </div>

      {note !== null && (
        <p aria-label="Import note" className="text-xs text-foreground">
          {note}
        </p>
      )}
      {error !== null && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}

      <div aria-label="Imported keys" className="flex flex-col gap-1">
        <h3 className="text-xs font-semibold text-foreground">Imported keys</h3>
        {keys.length === 0 ? (
          <p className="text-xs text-muted-foreground">No keys imported yet.</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {keys.map((row) => (
              <li
                key={row.id}
                className="flex items-center justify-between rounded-lg border border-border-subtle px-3 py-2 text-xs"
              >
                <span className="flex flex-col">
                  <span className="font-mono text-foreground">{row.fingerprint}</span>
                  <span className="text-muted-foreground">
                    {row.email ?? row.subjectCn ?? ''}
                  </span>
                </span>
                <span className="text-muted-foreground">
                  {row.standard} · {row.keyType}
                  {row.hasPrivate ? ' · private' : ''}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>

      <CollectedKeysReview account={chosen} />
    </section>
  );
}