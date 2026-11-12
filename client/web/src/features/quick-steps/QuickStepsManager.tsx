// SPDX-License-Identifier: GPL-3.0-only
// The quick steps manager: the saved macros listed with their edit and
// delete verbs, and an inline form to create or edit one (a name, the
// action, a plain folder select over the folder store for a move, and the
// mark-read-first checkbox). The five-cap disables add with its reason.
// The sweep dialog's chrome: store-gated, focus moves in on open, Escape
// and the backdrop close it.

import { useState } from 'react';

import { useDialogFocus } from '@/components/ui/useDialogFocus';
import { useFolderStore } from '@/stores/folderStore';

import {
  MAX_QUICK_STEPS,
  QUICK_STEP_ACTIONS,
  describeQuickStep,
  newQuickStepId,
  useQuickStepsStore,
  type QuickStep,
  type QuickStepAction,
  type QuickStepFolderTarget,
} from './quickStepsStore';

/** The form's working copy: the step being edited, or null while creating. */
interface Draft {
  editingId: string | null;
  name: string;
  action: QuickStepAction;
  target: QuickStepFolderTarget | null;
  markRead: boolean;
}

const EMPTY_DRAFT: Draft = {
  editingId: null,
  name: '',
  action: 'archive',
  target: null,
  markRead: false,
};

function draftFromStep(step: QuickStep): Draft {
  return {
    editingId: step.id,
    name: step.name,
    action: step.action,
    target: step.target ?? null,
    markRead: step.markRead === true,
  };
}

/** A step needs a name, and a move needs its destination. */
function draftComplete(draft: Draft): boolean {
  return draft.name.trim() !== '' && (draft.action !== 'move' || draft.target !== null);
}

/** The store gate: the dialog stands while the gate is open. */
export function QuickStepsManager() {
  const open = useQuickStepsStore((s) => s.managerOpen);
  if (!open) {
    return null;
  }
  return <QuickStepsManagerBody />;
}

/** The modal over the saved steps and the one form. */
function QuickStepsManagerBody() {
  const steps = useQuickStepsStore((s) => s.steps);
  const accounts = useFolderStore((s) => s.accounts);
  const [draft, setDraft] = useState<Draft | null>(null);
  const dialogRef = useDialogFocus<HTMLDivElement>();

  const atCap = steps.length >= MAX_QUICK_STEPS;
  const capReason = `You can save up to ${MAX_QUICK_STEPS} quick steps`;

  // Every account's folders in one flat select, each option carrying its
  // account and folder key as the value.
  const folderOptions = accounts.flatMap((account) =>
    account.folders.map((folder) => ({
      value: JSON.stringify({ account: account.account_id, folderKey: folder.key }),
      label: `${account.display_name}: ${folder.name}`,
    })),
  );

  const close = (): void => useQuickStepsStore.getState().closeManager();

  const saveDraft = (): void => {
    if (draft === null || !draftComplete(draft)) {
      return;
    }
    const step: QuickStep = {
      id: draft.editingId ?? newQuickStepId(),
      name: draft.name.trim(),
      action: draft.action,
    };
    if (draft.action === 'move' && draft.target !== null) {
      step.target = draft.target;
    }
    if (draft.markRead) {
      step.markRead = true;
    }
    useQuickStepsStore.getState().upsert(step);
    setDraft(null);
  };

  const remove = (id: string): void => {
    useQuickStepsStore.getState().delete(id);
    if (draft?.editingId === id) {
      setDraft(null);
    }
  };

  const field =
    'h-8 rounded-[var(--radius-xs)] border border-border bg-background px-2 text-xs text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';
  const small =
    'rounded-[var(--radius-xs)] px-2 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

  return (
    <div
      className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          close();
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Quick steps"
        ref={dialogRef}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            close();
          }
        }}
        className="flex max-h-full w-[480px] flex-col overflow-y-auto rounded-xl border border-border-subtle bg-card p-5 shadow-[var(--shadow-lg)] focus-visible:outline-none"
      >
        <h2 className="mb-1 text-sm font-semibold text-foreground">Quick steps</h2>
        <p className="mb-3 text-xs text-muted-text">
          Each step runs one action over the selected rows.
        </p>

        <ul className="flex flex-col gap-1.5">
          {steps.map((step) => (
            <li
              key={step.id}
              className="flex items-center gap-2 rounded-[var(--radius)] border border-border-subtle px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-xs font-medium text-foreground">{step.name}</div>
                <div className="truncate text-[11px] text-muted-text">{describeQuickStep(step)}</div>
              </div>
              <button
                type="button"
                aria-label={`Edit ${step.name}`}
                onClick={() => setDraft(draftFromStep(step))}
                className={`${small} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
              >
                Edit
              </button>
              <button
                type="button"
                aria-label={`Delete ${step.name}`}
                onClick={() => remove(step.id)}
                className={`${small} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
              >
                Delete
              </button>
            </li>
          ))}
          {steps.length === 0 && (
            <li className="rounded-[var(--radius)] border border-dashed border-border-subtle px-3 py-4 text-center text-xs text-muted-text">
              No quick steps yet.
            </li>
          )}
        </ul>

        {draft !== null ? (
          <form
            className="mt-3 flex flex-col gap-2 rounded-[var(--radius)] border border-border-subtle p-3"
            onSubmit={(event) => {
              event.preventDefault();
              saveDraft();
            }}
          >
            <input
              type="text"
              aria-label="Name"
              value={draft.name}
              onChange={(event) => setDraft({ ...draft, name: event.target.value })}
              placeholder="Clean inbox"
              autoFocus
              className={field}
            />
            <select
              aria-label="Action"
              value={draft.action}
              onChange={(event) =>
                setDraft({ ...draft, action: event.target.value as QuickStepAction })
              }
              className={field}
            >
              {QUICK_STEP_ACTIONS.map((action) => (
                <option key={action.id} value={action.id}>
                  {action.label}
                </option>
              ))}
            </select>
            {draft.action === 'move' && (
              <select
                aria-label="Folder"
                value={draft.target !== null ? JSON.stringify(draft.target) : ''}
                onChange={(event) =>
                  setDraft({
                    ...draft,
                    target:
                      event.target.value === ''
                        ? null
                        : (JSON.parse(event.target.value) as QuickStepFolderTarget),
                  })
                }
                className={field}
              >
                <option value="">Choose a folder…</option>
                {folderOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            )}
            <label className="flex items-center gap-2 text-xs text-foreground">
              <input
                type="checkbox"
                aria-label="Mark as read first"
                checked={draft.markRead}
                onChange={(event) => setDraft({ ...draft, markRead: event.target.checked })}
                className="h-3.5 w-3.5"
              />
              Mark as read first
            </label>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setDraft(null)}
                className={`${small} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={!draftComplete(draft)}
                className={`${small} bg-primary font-medium text-primary-foreground shadow-[var(--shadow-sm)] hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50`}
              >
                Save step
              </button>
            </div>
          </form>
        ) : (
          <button
            type="button"
            onClick={() => setDraft({ ...EMPTY_DRAFT })}
            disabled={atCap}
            title={atCap ? capReason : undefined}
            className={`${small} mt-3 self-start bg-primary font-medium text-primary-foreground shadow-[var(--shadow-sm)] hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50`}
          >
            Add step
          </button>
        )}

        <div className="mt-4 flex justify-end border-t border-border-subtle pt-3">
          <button
            type="button"
            onClick={close}
            className={`${small} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}