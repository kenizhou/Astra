// SPDX-License-Identifier: GPL-3.0-only
// The quick steps manager: the store gate, the saved list with its edit and
// delete verbs, the inline create/edit form (name, action, the folder select
// for move over the folder store, the mark-read checkbox), the five-cap that
// disables add with its reason, and the Escape/backdrop/Done dismissals. A
// mid-test store mutation under fake timers rides act().
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { QuickStepsManager } from './QuickStepsManager';
import { MAX_QUICK_STEPS, useQuickStepsStore, type QuickStep } from './quickStepsStore';
import { useFolderStore } from '@/stores/folderStore';
import type { AccountFolderRow } from '@/services/surfaces';

/** One step, overridable per test. */
function step(id: string, over: Partial<QuickStep> = {}): QuickStep {
  return { id, name: `Step ${id}`, action: 'archive', ...over };
}

/** One account group with two folders, the folder select's source. */
function accountGroup(): AccountFolderRow {
  return {
    account_id: 'work',
    display_name: 'Work',
    address: 'me@work.example',
    expanded: true,
    folders: [
      { key: 'INBOX', name: 'Inbox', role: 'inbox', unread: 0, depth: 0, has_children: false, expanded: true },
      { key: 'Receipts', name: 'Receipts', unread: 0, depth: 0, has_children: false, expanded: true },
    ],
  };
}

/** Seeds the saved steps and opens the dialog, as the Manage button does. */
function open(steps: QuickStep[] = []): void {
  useQuickStepsStore.setState({ steps });
  useQuickStepsStore.getState().openManager();
}

beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
  useQuickStepsStore.setState({ steps: [], managerOpen: false });
  useFolderStore.setState({ accounts: [accountGroup()] });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  localStorage.clear();
});

describe('the manager gate', () => {
  it('renders nothing while the gate is closed', () => {
    render(<QuickStepsManager />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('stands as a modal over the saved steps while the gate is open', () => {
    open([step('a'), step('b', { action: 'trash' })]);
    render(<QuickStepsManager />);

    const dialog = screen.getByRole('dialog', { name: 'Quick steps' });
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(within(dialog).getByText('Step a')).toBeTruthy();
    expect(within(dialog).getByText('Step b')).toBeTruthy();
    expect(within(dialog).getByText('Archive')).toBeTruthy();
    expect(within(dialog).getByText('Move to Trash')).toBeTruthy();
  });

  it('names the empty state while nothing is saved', () => {
    open();
    render(<QuickStepsManager />);
    expect(screen.getByText('No quick steps yet.')).toBeTruthy();
  });
});

describe('the create form', () => {
  it('creates a step from the name and action, then closes the form', () => {
    open();
    render(<QuickStepsManager />);

    fireEvent.click(screen.getByRole('button', { name: 'Add step' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Name' }), {
      target: { value: 'Clean inbox' },
    });
    fireEvent.change(screen.getByRole('combobox', { name: 'Action' }), {
      target: { value: 'trash' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Save step' }));

    const steps = useQuickStepsStore.getState().steps;
    expect(steps).toHaveLength(1);
    expect(steps[0]).toMatchObject({ name: 'Clean inbox', action: 'trash' });
    expect(steps[0]?.id).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Save step' })).toBeNull();
    expect(screen.getByText('Clean inbox')).toBeTruthy();
  });

  it('keeps Save disabled while the name is empty', () => {
    open();
    render(<QuickStepsManager />);

    fireEvent.click(screen.getByRole('button', { name: 'Add step' }));
    const save = screen.getByRole('button', { name: 'Save step' }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);

    fireEvent.change(screen.getByRole('textbox', { name: 'Name' }), {
      target: { value: 'Named' },
    });
    expect(save.disabled).toBe(false);
  });

  it('offers the folder select for move and requires a choice before saving', () => {
    open();
    render(<QuickStepsManager />);

    fireEvent.click(screen.getByRole('button', { name: 'Add step' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Name' }), {
      target: { value: 'File receipts' },
    });
    expect(screen.queryByRole('combobox', { name: 'Folder' })).toBeNull();

    fireEvent.change(screen.getByRole('combobox', { name: 'Action' }), {
      target: { value: 'move' },
    });
    const folder = screen.getByRole('combobox', { name: 'Folder' });
    expect(within(folder).getByRole('option', { name: 'Work: Receipts' })).toBeTruthy();
    const save = screen.getByRole('button', { name: 'Save step' }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);

    fireEvent.change(folder, {
      target: { value: JSON.stringify({ account: 'work', folderKey: 'Receipts' }) },
    });
    expect(save.disabled).toBe(false);
    fireEvent.click(save);

    expect(useQuickStepsStore.getState().steps[0]).toMatchObject({
      name: 'File receipts',
      action: 'move',
      target: { account: 'work', folderKey: 'Receipts' },
    });
  });

  it('stores the mark-read-first modifier when the checkbox stands', () => {
    open();
    render(<QuickStepsManager />);

    fireEvent.click(screen.getByRole('button', { name: 'Add step' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Name' }), {
      target: { value: 'Read then file' },
    });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Mark as read first' }));
    fireEvent.click(screen.getByRole('button', { name: 'Save step' }));

    expect(useQuickStepsStore.getState().steps[0]?.markRead).toBe(true);
  });

  it('Cancel closes the form without saving', () => {
    open();
    render(<QuickStepsManager />);

    fireEvent.click(screen.getByRole('button', { name: 'Add step' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Name' }), {
      target: { value: 'Never saved' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(useQuickStepsStore.getState().steps).toEqual([]);
    expect(screen.queryByRole('button', { name: 'Save step' })).toBeNull();
  });
});

describe('the edit and delete verbs', () => {
  it('edits a step in place, prefilling the form', () => {
    open([step('a', { name: 'Original', action: 'flag' })]);
    render(<QuickStepsManager />);

    fireEvent.click(screen.getByRole('button', { name: 'Edit Original' }));
    const name = screen.getByRole('textbox', { name: 'Name' }) as HTMLInputElement;
    expect(name.value).toBe('Original');
    expect((screen.getByRole('combobox', { name: 'Action' }) as HTMLSelectElement).value).toBe('flag');

    fireEvent.change(name, { target: { value: 'Renamed' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save step' }));

    const steps = useQuickStepsStore.getState().steps;
    expect(steps).toHaveLength(1);
    expect(steps[0]).toMatchObject({ id: 'a', name: 'Renamed', action: 'flag' });
  });

  it('prefills the move target and the modifier when editing a move step', () => {
    open([
      step('m', {
        name: 'File it',
        action: 'move',
        target: { account: 'work', folderKey: 'Receipts' },
        markRead: true,
      }),
    ]);
    render(<QuickStepsManager />);

    fireEvent.click(screen.getByRole('button', { name: 'Edit File it' }));
    const folder = screen.getByRole('combobox', { name: 'Folder' }) as HTMLSelectElement;
    expect(folder.value).toBe(JSON.stringify({ account: 'work', folderKey: 'Receipts' }));
    expect((screen.getByRole('checkbox', { name: 'Mark as read first' }) as HTMLInputElement).checked).toBe(true);
  });

  it('deletes a step from the list and the store', () => {
    open([step('a'), step('b')]);
    render(<QuickStepsManager />);

    fireEvent.click(screen.getByRole('button', { name: 'Delete Step a' }));

    expect(useQuickStepsStore.getState().steps.map((s) => s.id)).toEqual(['b']);
    expect(screen.queryByText('Step a')).toBeNull();
    expect(screen.getByText('Step b')).toBeTruthy();
  });
});

describe('the five-cap', () => {
  it('disables add with its reason at the limit', () => {
    const five = Array.from({ length: MAX_QUICK_STEPS }, (_, i) => step(`s${i}`));
    open(five);
    render(<QuickStepsManager />);

    const add = screen.getByRole('button', { name: 'Add step' }) as HTMLButtonElement;
    expect(add.disabled).toBe(true);
    expect(add.title).toBe('You can save up to 5 quick steps');
  });

  it('keeps add enabled below the limit and after a delete', () => {
    const five = Array.from({ length: MAX_QUICK_STEPS }, (_, i) => step(`s${i}`));
    open(five);
    render(<QuickStepsManager />);

    fireEvent.click(screen.getByRole('button', { name: 'Delete Step s0' }));
    const add = screen.getByRole('button', { name: 'Add step' }) as HTMLButtonElement;
    expect(add.disabled).toBe(false);
  });
});

describe('the dismissals', () => {
  it('Done closes the dialog through the gate', () => {
    open([step('a')]);
    render(<QuickStepsManager />);

    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(useQuickStepsStore.getState().managerOpen).toBe(false);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('Escape closes the dialog', () => {
    open();
    render(<QuickStepsManager />);

    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(useQuickStepsStore.getState().managerOpen).toBe(false);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('a backdrop click closes the dialog', () => {
    open();
    render(<QuickStepsManager />);

    const dialog = screen.getByRole('dialog');
    fireEvent.click(dialog.parentElement as HTMLElement);
    expect(useQuickStepsStore.getState().managerOpen).toBe(false);
  });

  it('re-renders when the saved steps change while the dialog stands', () => {
    open([step('a')]);
    render(<QuickStepsManager />);
    expect(screen.getByText('Step a')).toBeTruthy();

    act(() => {
      useQuickStepsStore.getState().upsert(step('b', { name: 'Fresh' }));
    });
    expect(screen.getByText('Fresh')).toBeTruthy();
  });
});