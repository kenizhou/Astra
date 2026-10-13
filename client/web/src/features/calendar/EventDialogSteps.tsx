// SPDX-License-Identifier: GPL-3.0-only
// The event dialog's question steps: the series warning that names what a
// series edit would discard, and the one-vs-all delete choice a repeating
// row asks before anything crosses.

/** The series edit confirm, naming the risk it gates. */
export function SeriesWarning({
  onBack,
  onConfirm,
}: {
  onBack: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-foreground">
        Editing every event discards the changes made to individual occurrences.
      </p>
      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onBack}
          className="rounded-[var(--radius)] border border-border-subtle px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground"
        >
          Back
        </button>
        <button
          type="button"
          onClick={onConfirm}
          className="rounded-[var(--radius)] bg-[var(--warning)] px-2.5 py-1 text-xs font-medium text-warning-foreground shadow-[var(--shadow-sm)]"
        >
          Edit every event
        </button>
      </div>
    </div>
  );
}

/** The one-vs-all delete choice on a repeating row. */
export function DeleteChoice({
  onBack,
  onDelete,
}: {
  onBack: () => void;
  onDelete: (occurrence: boolean) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-foreground">Delete one occurrence or the whole series?</p>
      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onBack}
          className="rounded-[var(--radius)] border border-border-subtle px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground"
        >
          Back
        </button>
        <button
          type="button"
          onClick={() => onDelete(true)}
          className="rounded-[var(--radius)] border border-[var(--destructive)] px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-hover"
        >
          Delete this event
        </button>
        <button
          type="button"
          onClick={() => onDelete(false)}
          className="rounded-[var(--radius)] bg-[var(--destructive)] px-2.5 py-1 text-xs font-medium text-destructive-foreground shadow-[var(--shadow-sm)]"
        >
          Delete all events
        </button>
      </div>
    </div>
  );
}
