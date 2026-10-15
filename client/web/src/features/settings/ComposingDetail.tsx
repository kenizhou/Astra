// SPDX-License-Identifier: GPL-3.0-only
// The Composing detail: the quote-style picker, whether the quoted
// original opens below the reply or above it. The composer seeds each
// reply from this setting; the arrangement the document then carries is
// the per-message truth.

import { dispatch, intents } from '@/services/ipc';

import { PERSIST_SENTENCE } from './GeneralDetail';
import { useSettingsStore } from './settingsStore';

/** The Composing page of the settings screen. */
export function ComposingDetail() {
  const settings = useSettingsStore((s) => s.settings);
  const error = useSettingsStore((s) => s.error);

  return (
    <section aria-label="Composing settings" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">Composing</h2>
      <label className="flex max-w-[280px] flex-col gap-1 text-xs text-muted-foreground">
        Quote style
        <select
          aria-label="Quote style"
          value={settings.quote_below ? 'below' : 'above'}
          onChange={(event) =>
            void dispatch(intents.setQuoteStyle(event.target.value === 'below'))
          }
          className="h-7 w-full rounded-full border border-input bg-background px-3 text-xs text-foreground outline-none transition-colors focus:border-primary"
        >
          <option value="below">Quote below the reply</option>
          <option value="above">Quote above the reply</option>
        </select>
      </label>
      {error === 'persist_failed' && (
        <p role="alert" className="text-xs text-[var(--warning)]">
          {PERSIST_SENTENCE}
        </p>
      )}
    </section>
  );
}
