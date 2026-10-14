// SPDX-License-Identifier: GPL-3.0-only
// The AI chip (UI1 D5): provenance on every chip, the five-state lifecycle
// (pending, generating, approve, approved, failed), and the approve action
// always explicit, matching the sidecar's fail-closed posture. Serves the
// reading pane, the composer toolbar, and the digest, only where the
// behaviour behind the chip is already live.

/** The chip's lifecycle states, in order. */
export type AiChipState = 'pending' | 'generating' | 'approve' | 'approved' | 'failed';

/** One AI offer with its provenance and its state. */
export function AiChip({
  label,
  provenance,
  state,
  onApprove,
  onRetry,
  approveLabel,
}: {
  label: string;
  provenance: string;
  state: AiChipState;
  onApprove?: () => void;
  onRetry?: () => void;
  /** The approve button's accessible name when the bare label is not enough. */
  approveLabel?: string;
}) {
  if (state === 'approved') {
    return (
      <span className="inline-flex min-h-[var(--hit)] items-center gap-1.5 px-3 text-xs text-muted-text">
        {label}
        <span aria-hidden="true" className="text-[var(--success)]">✓</span>
      </span>
    );
  }

  const frame =
    state === 'approve'
      ? 'border-primary bg-[var(--accent)]'
      : state === 'failed'
        ? 'border-[var(--warning)]'
        : 'border-dashed border-border-subtle text-muted-foreground';

  return (
    <span
      className={`inline-flex min-h-[var(--hit)] items-center gap-1.5 rounded-full border px-3 text-xs ${frame}`}
    >
      {state === 'generating' && (
        <span aria-hidden="true" className="h-2 w-2 animate-pulse rounded-full bg-primary" />
      )}
      {state === 'approve' && onApprove !== undefined ? (
        <button
          type="button"
          aria-label={approveLabel}
          onClick={onApprove}
          className="font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {label}
        </button>
      ) : (
        <span>{label}</span>
      )}
      <span className="text-[11px] text-muted-text">{provenance}</span>
      {state === 'failed' && onRetry !== undefined && (
        <button
          type="button"
          onClick={onRetry}
          className="font-medium text-[var(--warning)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          Retry
        </button>
      )}
    </span>
  );
}
