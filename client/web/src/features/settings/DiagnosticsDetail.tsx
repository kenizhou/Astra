// SPDX-License-Identifier: GPL-3.0-only
// The Diagnostics detail: the log tail viewer (the Read button pulling a
// screenful of lines, rendered monospace with the newest last), Copy for
// support (the tail beside the version, with the privacy note naming what
// logs never carry), Export over the host's save seam with its outcome
// surfaced, and the debug toggle, whose change lands with the next
// launch, the honest wording the note states. Every control reflects the
// pulled truth, never local optimism.

import { useState } from 'react';

import { dispatch, intents } from '@/services/ipc';
import { logExport, logTail, TAIL_LINES } from '@/services/diagnostics';

import { useSettingsStore } from './settingsStore';
import { currentStrings } from './strings';

/** One outcome sentence for a finished export, or null while none stands. */
function exportSentence(
  outcome: Awaited<ReturnType<typeof logExport>>,
  strings: ReturnType<typeof currentStrings>['diagnostics'],
): string | null {
  if (typeof outcome === 'string') {
    return null;
  }
  if ('saved' in outcome) {
    return strings.exportedTo(outcome.saved.path);
  }
  return strings.exportFailed(outcome.failed.reason);
}

/** The Diagnostics page of the settings screen. */
export function DiagnosticsDetail() {
  const settings = useSettingsStore((s) => s.settings);
  const about = useSettingsStore((s) => s.about);
  const [tail, setTail] = useState<string[] | null>(null);
  const [copied, setCopied] = useState<'idle' | 'done' | 'refused'>('idle');
  const [exported, setExported] = useState<string | null>(null);
  const strings = currentStrings().diagnostics;

  const read = (): void => {
    void logTail(TAIL_LINES).then(setTail);
  };

  const copy = (): void => {
    const pull = tail === null ? logTail(TAIL_LINES) : Promise.resolve(tail);
    void pull
      .then((lines) =>
        navigator.clipboard.writeText([...lines, `Astra Mail ${about.version}`].join('\n')),
      )
      .then(
        () => setCopied('done'),
        () => setCopied('refused'),
      );
  };

  const exportLog = (): void => {
    void logExport().then((outcome) => setExported(exportSentence(outcome, strings)));
  };

  return (
    <section aria-label="Diagnostics settings" className="flex flex-col gap-4">
      <h2 className="text-sm font-semibold text-foreground">{strings.title}</h2>
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={read}
          className="rounded-lg border border-border px-3 py-1.5 text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {strings.readButton}
        </button>
        <button
          type="button"
          onClick={copy}
          className="rounded-lg border border-border px-3 py-1.5 text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {strings.copyButton}
        </button>
        <button
          type="button"
          onClick={exportLog}
          className="rounded-lg border border-border px-3 py-1.5 text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {strings.exportButton}
        </button>
      </div>
      {tail !== null && (
        <pre
          aria-label={strings.tailLabel}
          className="max-h-[280px] overflow-auto rounded-[var(--radius-xs)] border border-border-subtle bg-background p-2 font-mono text-[11px] leading-relaxed text-foreground"
        >
          {tail.join('\n')}
        </pre>
      )}
      {copied === 'done' && (
        <p role="status" className="text-xs text-muted-foreground">
          {strings.copiedNote}
        </p>
      )}
      {copied === 'refused' && (
        <p role="alert" className="max-w-[420px] text-xs text-[var(--warning)]">
          {strings.copyRefusedNote}
        </p>
      )}
      {exported !== null && (
        <p role="status" className="max-w-[420px] break-all text-xs text-muted-foreground">
          {exported}
        </p>
      )}
      <div className="flex flex-col gap-1">
        <label className="flex items-center gap-2 text-xs text-foreground">
          <input
            type="checkbox"
            aria-label={strings.debugToggle}
            checked={settings.debug_enabled ?? false}
            onChange={(event) => void dispatch(intents.setDebugEnabled(event.target.checked))}
          />
          {strings.debugToggle}
        </label>
        <p className="text-xs text-muted-foreground">{strings.debugNote}</p>
      </div>
      <p className="max-w-[420px] text-xs text-muted-foreground">{strings.privacyNote}</p>
    </section>
  );
}
