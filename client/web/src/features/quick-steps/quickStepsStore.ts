// SPDX-License-Identifier: GPL-3.0-only
// The device-local quick steps: user-defined macros that run one mail action
// over the selected rows in a click, persisted to localStorage the way the
// notification history is. A step is one action plus an optional
// mark-read-first modifier; a move names one concrete folder of one account.
// The list is capped at five, malformed entries drop on parse, and a storage
// failure is logged, never thrown. The manager gate rides here too, the
// sweep store's own shape, so the ribbon opens the dialog through the store.

import { create } from 'zustand';

import { useFolderStore } from '@/stores/folderStore';

/** How many steps the list keeps; the manager disables add at the limit. */
export const MAX_QUICK_STEPS = 5;

const STORAGE_KEY = 'astra.quickSteps';

/** The acts a quick step runs: the verbs the intent surface already serves. */
export type QuickStepAction =
  | 'archive'
  | 'trash'
  | 'mark-read'
  | 'mark-unread'
  | 'flag'
  | 'unflag'
  | 'move';

/** A concrete move destination: one folder of one account, by their keys. */
export interface QuickStepFolderTarget {
  account: string;
  folderKey: string;
}

/** One saved macro: a name, one action, and the optional modifiers. */
export interface QuickStep {
  id: string;
  name: string;
  action: QuickStepAction;
  /** The move destination; present only on a move step. */
  target?: QuickStepFolderTarget;
  /** Marks the eligible rows read before the action runs. */
  markRead?: boolean;
}

/** The actions the manager select offers, in its order. */
export const QUICK_STEP_ACTIONS: ReadonlyArray<{ id: QuickStepAction; label: string }> = [
  { id: 'archive', label: 'Archive' },
  { id: 'trash', label: 'Move to Trash' },
  { id: 'mark-read', label: 'Mark as read' },
  { id: 'mark-unread', label: 'Mark as unread' },
  { id: 'flag', label: 'Flag' },
  { id: 'unflag', label: 'Unflag' },
  { id: 'move', label: 'Move to folder' },
];

const ACTION_IDS: readonly QuickStepAction[] = QUICK_STEP_ACTIONS.map((a) => a.id);

/** Parses one persisted entry; null when it is not well-formed. A malformed
 *  target or modifier invalidates the whole step, never half a macro. */
function parseStep(value: unknown): QuickStep | null {
  if (typeof value !== 'object' || value === null) {
    return null;
  }
  const { id, name, action, target, markRead } = value as Record<string, unknown>;
  if (typeof id !== 'string' || id === '') {
    return null;
  }
  if (typeof name !== 'string' || name.trim() === '') {
    return null;
  }
  if (typeof action !== 'string' || !ACTION_IDS.includes(action as QuickStepAction)) {
    return null;
  }
  const step: QuickStep = { id, name, action: action as QuickStepAction };
  if (target !== undefined) {
    if (typeof target !== 'object' || target === null) {
      return null;
    }
    const { account, folderKey } = target as Record<string, unknown>;
    if (typeof account !== 'string' || account === '' || typeof folderKey !== 'string' || folderKey === '') {
      return null;
    }
    step.target = { account, folderKey };
  }
  if (markRead !== undefined) {
    if (typeof markRead !== 'boolean') {
      return null;
    }
    step.markRead = markRead;
  }
  return step;
}

/** Parses the persisted blob defensively; malformed data reads as empty. */
function readSteps(): QuickStep[] {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (raw === null) {
    return [];
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed
      .map(parseStep)
      .filter((step): step is QuickStep => step !== null)
      .slice(0, MAX_QUICK_STEPS);
  } catch {
    return [];
  }
}

/** Persists the list; a storage failure is logged, never thrown. */
function persist(steps: QuickStep[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(steps));
  } catch (err) {
    console.error('quick-steps: the steps could not be persisted', err);
  }
}

interface QuickStepsState {
  /** The saved steps, in the order the ribbon draws them. */
  steps: QuickStep[];
  /** Whether the manager dialog stands. */
  managerOpen: boolean;
  /** Adds a step, or replaces the one carrying its id, capped at the limit. */
  upsert: (step: QuickStep) => void;
  /** Removes one step by id; an absent id changes nothing. */
  delete: (id: string) => void;
  /** Opens the manager dialog. */
  openManager: () => void;
  /** Closes the manager dialog. */
  closeManager: () => void;
}

export const useQuickStepsStore = create<QuickStepsState>((set) => ({
  steps: readSteps(),
  managerOpen: false,

  upsert: (step) =>
    set((state) => {
      const index = state.steps.findIndex((existing) => existing.id === step.id);
      const next = [...state.steps];
      if (index >= 0) {
        next[index] = step;
      } else {
        next.push(step);
      }
      // The manager gates add at the limit; the clamp is the last line for
      // a hand-edited blob or a stale surface.
      const steps = next.slice(0, MAX_QUICK_STEPS);
      persist(steps);
      return { steps };
    }),

  delete: (id) =>
    set((state) => {
      const steps = state.steps.filter((step) => step.id !== id);
      persist(steps);
      return { steps };
    }),

  openManager: () => set({ managerOpen: true }),
  closeManager: () => set({ managerOpen: false }),
}));

/** A fresh unique step id. */
export function newQuickStepId(): string {
  return crypto.randomUUID();
}

/** The destination's live display name from the folder store, falling back
 *  to the stored key when the folder has gone. */
function targetName(target: QuickStepFolderTarget): string {
  const account = useFolderStore
    .getState()
    .accounts.find((group) => group.account_id === target.account);
  return account?.folders.find((folder) => folder.key === target.folderKey)?.name ?? target.folderKey;
}

/** One-line human summary for the ribbon tooltip and the manager list. */
export function describeQuickStep(step: QuickStep): string {
  const label = QUICK_STEP_ACTIONS.find((a) => a.id === step.action)?.label ?? step.action;
  const verb =
    step.action === 'move' && step.target !== undefined
      ? `Move to ${targetName(step.target)}`
      : label;
  return step.markRead === true ? `${verb}, mark read first` : verb;
}