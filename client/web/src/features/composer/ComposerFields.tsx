// SPDX-License-Identifier: GPL-3.0-only
// The composer's header fields: From as read only text (the default send
// account, fixed this slice), To and Subject always visible, Cc and Bcc
// behind their toggles so the common new message stays one line tall. A
// toggle whose field the composition cannot carry is not offered at all.
// Every change crosses with its field name; the values themselves live in
// the composer's store.

interface ComposerFieldsProps {
  /** The sending account's display name, read only. */
  fromLabel: string;
  to: string;
  cc: string;
  bcc: string;
  subject: string;
  /** Whether the Cc toggle stands beside To. */
  ccOffered: boolean;
  /** Whether the Cc field is shown. */
  ccShown: boolean;
  /** Whether the Bcc toggle stands beside To. */
  bccOffered: boolean;
  /** Whether the Bcc field is shown. */
  bccShown: boolean;
  /** Carries one field's new value, named as the wire names it. */
  onField: (field: 'to' | 'cc' | 'bcc' | 'subject', value: string) => void;
  onToggleCc: () => void;
  onToggleBcc: () => void;
}

const fieldClass =
  'w-full rounded-[var(--radius-xs)] border border-border bg-background px-2 py-1 text-[13px] text-foreground outline-none transition-colors focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring';

export function ComposerFields({
  fromLabel,
  to,
  cc,
  bcc,
  subject,
  ccOffered,
  ccShown,
  bccOffered,
  bccShown,
  onField,
  onToggleCc,
  onToggleBcc,
}: ComposerFieldsProps) {
  return (
    <div className="flex shrink-0 flex-col gap-1.5 border-b border-border bg-surface px-3 py-2">
      <div className="flex items-center gap-2">
        <span className="w-12 shrink-0 text-right text-xs text-muted-foreground">From</span>
        <span className="min-w-0 flex-1 truncate text-[13px] text-foreground">{fromLabel}</span>
      </div>
      <div className="flex items-center gap-2">
        <label htmlFor="composer-to" className="w-12 shrink-0 text-right text-xs text-muted-foreground">
          To
        </label>
        <input
          id="composer-to"
          type="text"
          aria-label="To"
          value={to}
          onChange={(event) => onField('to', event.target.value)}
          className={fieldClass}
        />
        {ccOffered && (
          <button
            type="button"
            aria-pressed={ccShown}
            onClick={onToggleCc}
            className="shrink-0 rounded-[var(--radius-xs)] px-1.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {ccShown ? 'Hide Cc' : 'Show Cc'}
          </button>
        )}
        {bccOffered && (
          <button
            type="button"
            aria-pressed={bccShown}
            onClick={onToggleBcc}
            className="shrink-0 rounded-[var(--radius-xs)] px-1.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {bccShown ? 'Hide Bcc' : 'Show Bcc'}
          </button>
        )}
      </div>
      {ccShown && (
        <div className="flex items-center gap-2">
          <label htmlFor="composer-cc" className="w-12 shrink-0 text-right text-xs text-muted-foreground">
            Cc
          </label>
          <input
            id="composer-cc"
            type="text"
            aria-label="Cc"
            value={cc}
            onChange={(event) => onField('cc', event.target.value)}
            className={fieldClass}
          />
        </div>
      )}
      {bccShown && (
        <div className="flex items-center gap-2">
          <label htmlFor="composer-bcc" className="w-12 shrink-0 text-right text-xs text-muted-foreground">
            Bcc
          </label>
          <input
            id="composer-bcc"
            type="text"
            aria-label="Bcc"
            value={bcc}
            onChange={(event) => onField('bcc', event.target.value)}
            className={fieldClass}
          />
        </div>
      )}
      <div className="flex items-center gap-2">
        <label htmlFor="composer-subject" className="w-12 shrink-0 text-right text-xs text-muted-foreground">
          Subject
        </label>
        <input
          id="composer-subject"
          type="text"
          aria-label="Subject"
          value={subject}
          onChange={(event) => onField('subject', event.target.value)}
          className={fieldClass}
        />
      </div>
    </div>
  );
}
