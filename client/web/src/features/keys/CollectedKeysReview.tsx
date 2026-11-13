// SPDX-License-Identifier: GPL-3.0-only
// The collected-key review (SP4d-2 D2): the staged keys a discovery source
// (Autocrypt, gossip, WKD) observed but the person has not yet accepted. Each
// row shows the fingerprint, peer email, source, and standard, and offers
// Accept (unverified), Accept as verified, and Reject. The fingerprint is shown
// so a person can verify it out of band before choosing Accept as verified.
// Accept promotes the key into the keystore paired with the trust decision that
// makes it send-usable; Reject removes the staging row and records a rejected
// decision. Collected keys are public material: no secret crosses this surface.

import { useCallback, useEffect, useState } from 'react';

import {
  listCollected,
  promoteCollected,
  rejectCollected,
  type CollectedKeyRow,
} from '@/services/crypto';

const actionClass =
  'rounded-lg border border-border px-2.5 py-1 text-xs text-foreground transition-colors hover:bg-hover disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

/** The collected-key review list for one account, joined to the Keys detail. */
export function CollectedKeysReview({ account }: { account: string }) {
  const [rows, setRows] = useState<CollectedKeyRow[]>([]);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async (target: string): Promise<void> => {
    if (target === '') {
      setRows([]);
      return;
    }
    try {
      setRows(await listCollected(target));
    } catch (err) {
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    void refresh(account);
  }, [account, refresh]);

  // The accept seam: promote the staged key with the trust decision (verified
  // for the out-of-band choice, unverified otherwise), then refresh the list.
  const accept = async (row: CollectedKeyRow, verified: boolean): Promise<void> => {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      await promoteCollected(
        account,
        row.standard ?? '',
        row.fingerprint ?? '',
        row.peerEmail ?? null,
        verified,
      );
      setNote(`Accepted ${row.fingerprint ?? ''}`);
      await refresh(account);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  // The reject seam: remove the staging row and record the rejected decision.
  const reject = async (row: CollectedKeyRow): Promise<void> => {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      await rejectCollected(account, row.id, row.standard ?? '', row.fingerprint ?? '', row.peerEmail ?? null);
      setNote(`Rejected ${row.fingerprint ?? ''}`);
      await refresh(account);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div aria-label="Collected keys" className="flex flex-col gap-2">
      <h3 className="text-xs font-semibold text-foreground">Collected keys</h3>
      <p className="max-w-[520px] text-xs text-muted-foreground">
        Keys observed from incoming mail await your review. Check the fingerprint
        out of band before accepting as verified; accepting without verification
        still lets you send encrypted mail to the peer.
      </p>

      {note !== null && (
        <p aria-label="Review note" className="text-xs text-foreground">
          {note}
        </p>
      )}
      {error !== null && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}

      {rows.length === 0 ? (
        <p className="text-xs text-muted-foreground">No collected keys to review.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {rows.map((row) => (
            <li
              key={row.id}
              className="flex flex-col gap-2 rounded-lg border border-border-subtle px-3 py-2 text-xs"
            >
              <span className="flex flex-col">
                <span className="font-mono text-foreground">{row.fingerprint}</span>
                <span className="text-muted-foreground">{row.peerEmail ?? ''}</span>
                <span className="text-muted-foreground">
                  {row.source ?? ''} · {row.standard ?? ''}
                </span>
              </span>
              <span className="flex items-center gap-2">
                <button
                  type="button"
                  aria-label={`Accept ${row.fingerprint ?? ''}`}
                  disabled={busy}
                  onClick={() => void accept(row, false)}
                  className={actionClass}
                >
                  Accept
                </button>
                <button
                  type="button"
                  aria-label={`Accept as verified ${row.fingerprint ?? ''}`}
                  disabled={busy}
                  onClick={() => void accept(row, true)}
                  className={actionClass}
                >
                  Accept as verified
                </button>
                <button
                  type="button"
                  aria-label={`Reject ${row.fingerprint ?? ''}`}
                  disabled={busy}
                  onClick={() => void reject(row)}
                  className={actionClass}
                >
                  Reject
                </button>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}