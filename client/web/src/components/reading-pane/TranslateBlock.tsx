// SPDX-License-Identifier: GPL-3.0-only
// The translate block: the language select over the fixed six with English
// standing selected, the on-demand run into the chosen language with its
// loading state, the result in a collapsible block a cached instant return
// re-expands without a second call, and the failure sentence beside the
// retry the standing action offers. The wire's own carry binds this block:
// the host answers an empty string for both a message it cannot resolve
// and a failed call, so the empty answer is the error state here, never a
// silent blank. The block resets with each newly opened message and stands
// absent with nothing open. Each run settles only against the block it
// started for, the composer's send-epoch shape.

import { useEffect, useRef, useState } from 'react';

import { translateMessage, type TranslateLanguage } from '@/services/ai';
import { useReadingStore } from '@/stores/readingStore';

/** Where one translate run stands. */
type Phase = 'idle' | 'loading' | 'done' | 'error';

/** The six target languages, their wire words beside their labels. */
const LANGUAGES: { value: TranslateLanguage; label: string }[] = [
  { value: 'english', label: 'English' },
  { value: 'chinese', label: 'Chinese' },
  { value: 'german', label: 'German' },
  { value: 'french', label: 'French' },
  { value: 'japanese', label: 'Japanese' },
  { value: 'spanish', label: 'Spanish' },
];

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

export function TranslateBlock() {
  const message = useReadingStore((s) => s.snapshot?.message ?? null);
  const account = message?.account;
  const key = message?.key;
  const [lang, setLang] = useState<TranslateLanguage>('english');
  const [phase, setPhase] = useState<Phase>('idle');
  const [translation, setTranslation] = useState('');
  const [translatedAs, setTranslatedAs] = useState<TranslateLanguage>('english');
  const [error, setError] = useState('');
  const [expanded, setExpanded] = useState(true);
  // The run epoch: bumped by every message switch and every click, so each
  // run settles only while it is still the newest, never under a message the
  // reader has moved on to.
  const runEpoch = useRef(0);

  useEffect(() => {
    runEpoch.current += 1;
    setLang('english');
    setPhase('idle');
    setTranslation('');
    setTranslatedAs('english');
    setError('');
    setExpanded(true);
  }, [account, key]);

  if (message === null) {
    return null;
  }

  const label = LANGUAGES.find((entry) => entry.value === translatedAs)?.label ?? '';

  const translate = (): void => {
    const run = ++runEpoch.current;
    setPhase('loading');
    translateMessage(message, lang).then(
      (text) => {
        if (runEpoch.current !== run) {
          return;
        }
        if (text === '') {
          // The wire's empty carry: a run that cannot be named still fails
          // in words, with the standing action as its retry.
          setError('');
          setPhase('error');
          return;
        }
        setTranslation(text);
        setTranslatedAs(lang);
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

  return (
    <section aria-label="Translate" className="shrink-0 border-b border-border-subtle px-4 py-2">
      {phase === 'error' && (
        <p role="alert" className="mb-1 text-xs text-muted-foreground">
          {error === ''
            ? 'The translation is unavailable.'
            : `The translation is unavailable: ${error}`}
        </p>
      )}
      <div className="flex items-center gap-1">
        <select
          aria-label="Translate to"
          value={lang}
          onChange={(event) => setLang(event.target.value as TranslateLanguage)}
          disabled={phase === 'loading'}
          className="rounded-full border border-border bg-background px-2.5 py-1 text-xs text-foreground outline-none transition-colors focus-visible:border-primary"
        >
          {LANGUAGES.map((entry) => (
            <option key={entry.value} value={entry.value}>
              {entry.label}
            </option>
          ))}
        </select>
        <button
          type="button"
          disabled={phase === 'loading'}
          onClick={translate}
          className="rounded-[var(--radius-xs)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-50"
        >
          {phase === 'loading' ? 'Translating' : 'Translate'}
        </button>
      </div>
      {phase === 'done' && (
        <div className="mt-1">
          <div className="flex items-center gap-1">
            <span className="text-xs font-medium text-foreground">{`Translation (${label})`}</span>
            <button
              type="button"
              aria-label={expanded ? 'Hide translation' : 'Show translation'}
              aria-expanded={expanded}
              onClick={() => setExpanded((open) => !open)}
              className="rounded-[var(--radius-xs)] p-0.5 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <ChevronGlyph expanded={expanded} />
            </button>
          </div>
          {expanded && <p className="mt-1 text-xs leading-relaxed text-foreground">{translation}</p>}
        </div>
      )}
    </section>
  );
}
