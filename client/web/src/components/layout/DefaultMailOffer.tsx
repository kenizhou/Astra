// SPDX-License-Identifier: GPL-3.0-only
// The default-mail offer banner: asked once, only when an account stands
// and the host's latch file is unset (the ask-once discipline lives
// host-side in the app data dir). Both answers mark the ask spent: the
// acceptance re-asserts the registration through the host verb and keeps
// the banner up for its outcome sentence, the pass takes the banner down.
// A refused latch read never nags: the banner treats an unknown latch as
// asked.

import { useEffect, useState } from 'react';

import { isTauri } from '@/events/surfaceEvents';
import {
  defaultMailAsked,
  markDefaultMailAsked,
  registerDefaultMail,
  type RegisterDefaultMailOutcome,
} from '@/services/platform';
import { useAccountStore } from '@/stores/accountStore';

/** One outcome as the sentence the banner shows beside a close control. */
function outcomeSentence(outcome: RegisterDefaultMailOutcome): string {
  if (outcome === 'registered') {
    return 'Astra Mail is the default mail app.';
  }
  if (outcome === 'already_default') {
    return 'Astra Mail already is the default mail app.';
  }
  return outcome.failed.reason;
}

/** The ask-once default-mail offer over the shell. */
export function DefaultMailOffer() {
  const accounts = useAccountStore((s) => s.accounts);
  // null while the latch is unread, false once the ask may stand.
  const [mayAsk, setMayAsk] = useState(false);
  const [asking, setAsking] = useState(true);
  const [outcome, setOutcome] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri || accounts.length === 0 || !asking) {
      return;
    }
    let standing = true;
    void defaultMailAsked().then(
      (asked) => {
        if (standing) {
          // Only an explicit false may ask: a null or odd answer counts as
          // asked, so the banner never nags on a seam it cannot read.
          setMayAsk(asked === false);
          setAsking(false);
        }
      },
      () => {
        // A refused read never nags: the latch counts as asked.
        if (standing) {
          setMayAsk(false);
          setAsking(false);
        }
      },
    );
    return () => {
      standing = false;
    };
  }, [accounts.length, asking]);

  if (!isTauri || accounts.length === 0 || (!mayAsk && outcome === null)) {
    return null;
  }

  const pass = (): void => {
    setMayAsk(false);
    setOutcome(null);
    void markDefaultMailAsked().catch(() => undefined);
  };

  const accept = (): void => {
    void markDefaultMailAsked().catch(() => undefined);
    void registerDefaultMail().then(
      (result) => setOutcome(outcomeSentence(result)),
      (err: unknown) =>
        setOutcome(err instanceof Error ? err.message : 'The registration failed.'),
    );
    setMayAsk(false);
  };

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-10 z-[var(--z-modal)] flex justify-center px-4">
      <div
        role="dialog"
        aria-label="Default mail app"
        className="pointer-events-auto flex max-w-[520px] items-center gap-3 rounded-lg border border-border-subtle bg-card px-3 py-2 text-xs shadow-lg"
      >
        {outcome === null ? (
          <>
            <span className="min-w-0 flex-1 text-foreground">
              Make Astra Mail the app mail links open?
            </span>
            <button
              type="button"
              onClick={accept}
              className="whitespace-nowrap rounded-[var(--radius-xs)] bg-primary px-2 py-1 font-medium text-primary-foreground transition-colors hover:bg-primary-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Use Astra Mail for mail links
            </button>
            <button
              type="button"
              onClick={pass}
              className="whitespace-nowrap rounded-[var(--radius-xs)] px-2 py-1 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Not now
            </button>
          </>
        ) : (
          <>
            <span className="min-w-0 flex-1 text-foreground">{outcome}</span>
            <button
              type="button"
              aria-label="Dismiss default mail outcome"
              onClick={pass}
              className="whitespace-nowrap rounded-[var(--radius-xs)] px-2 py-1 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              Done
            </button>
          </>
        )}
      </div>
    </div>
  );
}
