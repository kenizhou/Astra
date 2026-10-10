// SPDX-License-Identifier: GPL-3.0-only
// The summarise block: the button that runs the cached summarise action for
// the open message, the loading state while the run is in flight, the
// failure sentence a refused run surfaces beside its retry, and the summary
// itself in a collapsible block that a cached instant return re-expands
// without a second call. The block resets with each newly opened message and
// stands absent with nothing open. Each run settles only against the block
// it started for (the composer's send-epoch shape): a run the reader
// switched away from lands neither its summary nor its failure.

import { useEffect, useRef, useState } from 'react';

import { summariseMessage } from '@/services/ai';
import { useReadingStore } from '@/stores/readingStore';

/** Where one summarise run stands. */
type Phase = 'idle' | 'loading' | 'done' | 'error';

/** A stroke-drawn chevron that points down while the block is expanded. */
function ChevronGlyph({ expanded }: { expanded: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width="11"
      height="11"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      style={{ transform: expanded ? 'rotate(180deg)' : 'none' }}
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

/** The rejection the host crosses, as a sentence to show. */
function sentenceOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function SummaryBlock() {
  const message = useReadingStore((s) => s.snapshot?.message ?? null);
  const account = message?.account;
  const key = message?.key;
  const [phase, setPhase] = useState<Phase>('idle');
  const [summary, setSummary] = useState('');
  const [error, setError] = useState('');
  const [expanded, setExpanded] = useState(true);
  // The run epoch: bumped by every message switch and every click, so each
  // run settles only while it is still the newest, never under a message the
  // reader has moved on to.
  const runEpoch = useRef(0);

  useEffect(() => {
    runEpoch.current += 1;
    setPhase('idle');
    setSummary('');
    setError('');
    setExpanded(true);
  }, [account, key]);

  if (message === null) {
    return null;
  }

  const summarise = (): void => {
    const run = ++runEpoch.current;
    setPhase('loading');
    summariseMessage(message).then(
      (text) => {
        if (runEpoch.current !== run) {
          return;
        }
        setSummary(text);
        setPhase('done');
      },
      (err) => {
        if (runEpoch.current !== run) {
          return;
        }
        setError(sentenceOf(err));
        setPhase('error');
      },
    );
  };

  if (phase === 'done') {
    return (
      <section
        aria-label="Message summary"
        className="shrink-0 border-b border-border px-4 py-2"
      >
        <div className="flex items-center gap-1">
          <span className="text-xs font-medium text-foreground">Summary</span>
          <button
            type="button"
            aria-label={expanded ? 'Hide summary' : 'Show summary'}
            aria-expanded={expanded}
            onClick={() => setExpanded((open) => !open)}
            className="rounded-[var(--radius-xs)] p-0.5 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <ChevronGlyph expanded={expanded} />
          </button>
        </div>
        {expanded && <p className="mt-1 text-xs leading-relaxed text-foreground">{summary}</p>}
      </section>
    );
  }

  return (
    <section aria-label="Summarise" className="shrink-0 border-b border-border px-4 py-2">
      {phase === 'error' && (
        <p role="alert" className="mb-1 text-xs text-muted-foreground">
          The summary is unavailable: {error}
        </p>
      )}
      <button
        type="button"
        disabled={phase === 'loading'}
        onClick={summarise}
        className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-50"
      >
        {phase === 'loading' ? 'Summarising' : 'Summarise'}
      </button>
    </section>
  );
}
