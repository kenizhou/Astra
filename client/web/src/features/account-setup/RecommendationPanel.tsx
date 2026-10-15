// SPDX-License-Identifier: GPL-3.0-only
// The recommendation step: the found settings as the headline, and the trust
// decision named beside them when the discovery crossed an unencrypted hop.
// The approve action is the primary button. The trust rule itself stands: an
// untrusted discovery's settings reach the form only through the approve
// click; entering servers by hand never carries them.

import { useState } from 'react';

import type { RecommendationView, ServerSummary, SetupChoice } from '@/services/ipc';

import { recommendationEmail } from './detection';

interface RecommendationPanelProps {
  view: RecommendationView;
  /** Returns to the email step. */
  onBack: () => void;
  /** Opens the manual form on the bare defaults, nothing discovered carried. */
  onByHand: () => void;
  /** Continues with the discovered settings onto the manual form. */
  onManual: () => void;
}

const secondaryClass =
  'shrink-0 whitespace-nowrap rounded-[var(--radius-xs)] px-3 py-1.5 text-[13px] text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';
const primaryClass =
  'shrink-0 whitespace-nowrap rounded-[var(--radius-xs)] bg-primary px-3 py-1.5 text-[13px] text-primary-foreground transition-opacity hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

/** The discovery strategies as the sub-line names them. */
const SOURCE_WORDS: Record<string, string> = {
  JmapWellKnown: "the server's well-known document",
  JmapSrv: "the domain's SRV records",
  Autoconfig: "the provider's own records",
  AutoconfigWellKnown: "the provider's own records",
  Ispdb: 'the Mozilla ISP database',
  ImapSrv: "the domain's SRV records",
  MxAutoconfig: 'a lookup from the MX record',
  MxIspdb: 'a lookup from the MX record',
};

/** One detection row: what it is, what was found, the wire detail. */
function DetectRow({ label, value, right }: { label: string; value: string; right?: string }) {
  return (
    <div className="flex items-baseline justify-between gap-2 rounded-[var(--radius-xs)] border border-border bg-background px-3 py-2">
      <span className="w-16 shrink-0 text-xs text-muted-foreground">{label}</span>
      <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-foreground">{value}</span>
      {right !== undefined && (
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{right}</span>
      )}
    </div>
  );
}

function serverRight(server: ServerSummary): string {
  return `${server.protocol} · ${server.port} · ${server.security}`;
}

/** The rows one recommendation shows for what it found. */
function FoundRows({ view }: { view: RecommendationView }) {
  const rec = view.recommendation;
  if ('Imap' in rec) {
    const imap = rec.Imap;
    return (
      <div className="grid gap-1">
        <DetectRow
          label="Incoming"
          value={imap.incoming.hostname}
          right={serverRight(imap.incoming)}
        />
        {imap.outgoing !== undefined && (
          <DetectRow
            label="Outgoing"
            value={imap.outgoing.hostname}
            right={serverRight(imap.outgoing)}
          />
        )}
        <DetectRow
          label="Username"
          value={imap.incoming.username === '' ? imap.email : imap.incoming.username}
        />
      </div>
    );
  }
  if ('Jmap' in rec) {
    return <DetectRow label="JMAP" value={rec.Jmap.server_url} />;
  }
  if ('Microsoft' in rec) {
    return <p className="text-[13px]">Microsoft account</p>;
  }
  if ('Google' in rec) {
    return <p className="text-[13px]">Google account</p>;
  }
  return <p className="text-[13px]">Calendar and contacts</p>;
}

/** The unencrypted hop, named from the discovery's own source. */
function hopName(source: string): string {
  try {
    return `the settings document from ${new URL(source).host}`;
  } catch {
    return source === '' ? 'the discovery lookup' : `the ${source} lookup`;
  }
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

export function RecommendationPanel({ view, onBack, onByHand, onManual }: RecommendationPanelProps) {
  const [choices, setChoices] = useState<SetupChoice[]>(() =>
    view.choices.map((choice) => ({ ...choice })),
  );

  const rec = view.recommendation;
  const isImap = 'Imap' in rec;
  const needsApproval = isImap && !rec.Imap.is_trusted;
  const email = recommendationEmail(view);
  const sourceKind = view.detected.source_kind;
  const headline =
    'Google' in rec
      ? 'This address signs in with Google'
      : 'Microsoft' in rec
        ? 'This address signs in with Microsoft'
        : 'We found your settings';
  const subline =
    sourceKind !== null && sourceKind in SOURCE_WORDS
      ? `${email} · discovered from ${SOURCE_WORDS[sourceKind]}`
      : email;

  const toggle = (index: number) => () => {
    setChoices((prev) =>
      prev.map((choice, at) => (at === index ? { ...choice, on: !choice.on } : choice)),
    );
  };

  return (
    <div className="grid gap-3">
      <div className="grid gap-0.5">
        <h3 className="text-sm font-semibold text-foreground">{headline}</h3>
        <p className="text-xs text-muted-foreground">{subline}</p>
      </div>
      <FoundRows view={view} />

      {needsApproval && 'Imap' in rec && (
        <div className="grid gap-1.5 rounded-[var(--radius-xs)] border border-warning bg-accent p-3 text-xs">
          <p className="text-foreground">
            One lookup ({hopName(rec.Imap.source)}) came back over an unencrypted connection.
          </p>
          <p className="text-foreground">
            The mail servers themselves are encrypted end to end.{' '}
            <span className="font-medium">Continue with these settings?</span>
          </p>
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
        The next step collects the password and tests the connection before anything is stored.
      </p>

      <div className="mt-1 flex items-center justify-between gap-2">
        {isImap ? (
          <button type="button" onClick={onByHand} className={secondaryClass}>
            Enter servers by hand
          </button>
        ) : (
          <span />
        )}
        <div className="flex items-center gap-2">
          <button type="button" onClick={onBack} className={secondaryClass}>
            Back
          </button>
          <button type="button" onClick={onManual} className={primaryClass}>
            {needsApproval ? 'Approve and continue' : 'Continue'}
          </button>
        </div>
      </div>
    </div>
  );
}
