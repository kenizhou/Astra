// SPDX-License-Identifier: GPL-3.0-only
// The recommendation panel: what detection found for the address, the uses it
// offers as toggles, and the one way this build continues, onto the manual
// form carrying whatever the recommendation could prefill. A recommendation
// discovered over an unencrypted hop continues only after explicit approval.

import { useState } from 'react';

import type { RecommendationView, ServerSummary, SetupChoice } from '@/services/ipc';

interface RecommendationPanelProps {
  view: RecommendationView;
  onCancel: () => void;
  onManual: () => void;
}

function ServerRow({ server }: { server: ServerSummary }) {
  return (
    <div className="flex items-baseline justify-between gap-2 rounded-[var(--radius-xs)] border border-border bg-background px-3 py-2">
      <span className="text-xs text-muted-foreground">{server.protocol}</span>
      <span className="flex items-baseline gap-2 text-[13px]">
        <span className="font-mono">{server.hostname}</span>
        <span>{server.port}</span>
        <span className="text-muted-foreground">{server.security}</span>
      </span>
    </div>
  );
}

/** The one line each recommendation route shows for what it found. */
function FoundLine({ view }: { view: RecommendationView }) {
  const rec = view.recommendation;
  if ('Imap' in rec) {
    const imap = rec.Imap;
    return (
      <div className="grid gap-1">
        <ServerRow server={imap.incoming} />
        {imap.outgoing !== undefined && <ServerRow server={imap.outgoing} />}
      </div>
    );
  }
  if ('Jmap' in rec) {
    return (
      <p className="text-[13px]">
        <span className="text-muted-foreground">JMAP</span>{' '}
        <span className="font-mono">{rec.Jmap.server_url}</span>
      </p>
    );
  }
  if ('Microsoft' in rec) {
    return <p className="text-[13px]">Microsoft account</p>;
  }
  if ('Google' in rec) {
    return <p className="text-[13px]">Google account</p>;
  }
  return <p className="text-[13px]">Calendar and contacts</p>;
}

function ChoiceRow({ choice, onToggle }: { choice: SetupChoice; onToggle: () => void }) {
  return (
    <div className="flex items-center justify-between gap-2 text-[13px]">
      <label className="flex items-center gap-2">
        {choice.capability}
        <input
          type="checkbox"
          checked={choice.on}
          onChange={onToggle}
          className="size-4 accent-[var(--primary)]"
        />
      </label>
      <span className="text-xs text-muted-foreground">
        {choice.server_found ? 'server found' : 'not found'}
      </span>
    </div>
  );
}

export function RecommendationPanel({ view, onCancel, onManual }: RecommendationPanelProps) {
  const [choices, setChoices] = useState<SetupChoice[]>(() =>
    view.choices.map((choice) => ({ ...choice })),
  );
  const [approved, setApproved] = useState(false);

  const rec = view.recommendation;
  const needsApproval = 'Imap' in rec && !rec.Imap.is_trusted;
  const canContinue = !needsApproval || approved;

  const toggle = (index: number) => () => {
    setChoices((prev) =>
      prev.map((choice, at) => (at === index ? { ...choice, on: !choice.on } : choice)),
    );
  };

  return (
    <div className="grid gap-3">
      <p className="text-xs text-muted-foreground">Recommended setup</p>
      <FoundLine view={view} />

      {needsApproval && (
        <div className="grid gap-2 rounded-[var(--radius-xs)] border border-warning bg-accent p-3 text-xs">
          <p className="font-medium text-warning">
            These settings were discovered over an unencrypted connection and must be approved
            before use.
          </p>
          <label className="flex items-center gap-2 text-[13px] text-foreground">
            <input
              type="checkbox"
              checked={approved}
              onChange={() => setApproved((prev) => !prev)}
              className="size-4 accent-[var(--primary)]"
            />
            I trust these settings
          </label>
        </div>
      )}

      {choices.length > 0 && (
        <div className="grid gap-2 rounded-[var(--radius-xs)] border border-border p-3">
          <p className="text-xs text-muted-foreground">Use this account for</p>
          {choices.map((choice, index) => (
            <ChoiceRow key={choice.capability} choice={choice} onToggle={toggle(index)} />
          ))}
        </div>
      )}

      <p className="text-xs text-muted-foreground">
        The manual form carries these settings over; the connection is still tested before
        anything is stored.
      </p>

      <div className="mt-1 flex items-center justify-between gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="rounded-[var(--radius-xs)] px-3 py-1.5 text-[13px] text-muted-foreground hover:bg-hover hover:text-foreground"
        >
          Cancel
        </button>
        <button
          type="button"
          onClick={onManual}
          disabled={!canContinue}
          className="rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-40"
        >
          Set up manually
        </button>
      </div>
    </div>
  );
}
