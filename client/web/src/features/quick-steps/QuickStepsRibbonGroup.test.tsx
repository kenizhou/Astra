// SPDX-License-Identifier: GPL-3.0-only
// The quick steps ribbon group: one button per saved step labelled by its
// name, disabled without a selection, running the step over the selection
// (the multi-selection or the single anchor), the Manage button opening the
// store-gated dialog, and the compact collapse. The ReadRibbon mount stands
// the group beside the other three. A mid-test store mutation under fake
// timers rides act().
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
}));

import { QuickStepsRibbonGroup } from './QuickStepsRibbonGroup';
import { useQuickStepsStore, type QuickStep } from './quickStepsStore';
import { ReadRibbon } from '@/components/ribbon/ReadRibbon';
import { useListStore } from '@/stores/listStore';
import type { MessageRowView } from '@/services/surfaces';

/** One flat message row, overridable per test. */
function row(key: string, over: Partial<MessageRowView> = {}): MessageRowView {
  return {
    key: { message: { account: 'work', key } },
    account_id: 'work',
    sender_name: 'Ada Lovelace',
    sender_address: 'ada@example.test',
    subject: `Subject ${key}`,
    sent_at: '2026-11-12T08:30:00Z',
    unread: true,
    flagged: false,
    has_attachments: false,
    ...over,
  };
}

const a1 = row('a1');
const a2 = row('a2');

/** One step, overridable per test. */
function step(id: string, over: Partial<QuickStep> = {}): QuickStep {
  return { id, name: `Step ${id}`, action: 'archive', ...over };
}

beforeEach(() => {
  vi.useFakeTimers();
  localStorage.clear();
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  useQuickStepsStore.setState({ steps: [], managerOpen: false });
  useListStore.setState({ rows: [], selectedKeys: [], selectedKey: null, anchorKey: null, search: null });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  localStorage.clear();
});

describe('the quick steps ribbon group', () => {
  it('renders one button per saved step, labelled by name', () => {
    useQuickStepsStore.setState({ steps: [step('a', { name: 'Clean inbox' }), step('b', { name: 'File receipts', action: 'move', target: { account: 'work', folderKey: 'Receipts' } })] });
    render(<QuickStepsRibbonGroup iconOnly={false} />);

    expect(screen.getByRole('button', { name: 'Clean inbox' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'File receipts' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Manage quick steps' })).toBeTruthy();
  });

  it('shows only Manage while nothing is saved', () => {
    render(<QuickStepsRibbonGroup iconOnly={false} />);
    expect(screen.queryByRole('button', { name: 'Manage quick steps' })).toBeTruthy();
    expect(screen.getAllByRole('button')).toHaveLength(1);
  });

  it('disables the step buttons without a selection and dispatches nothing', () => {
    useQuickStepsStore.setState({ steps: [step('a')] });
    useListStore.setState({ rows: [a1] });
    render(<QuickStepsRibbonGroup iconOnly={false} />);

    const button = screen.getByRole('button', { name: 'Step a' }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);

    fireEvent.click(button);
    expect(ipc.dispatch).not.toHaveBeenCalled();
  });

  it('runs the step over the single anchor when no multi-selection stands', () => {
    useQuickStepsStore.setState({ steps: [step('a', { action: 'trash' })] });
    useListStore.setState({ rows: [a1, a2], selectedKey: a2.key });
    render(<QuickStepsRibbonGroup iconOnly={false} />);

    const button = screen.getByRole('button', { name: 'Step a' }) as HTMLButtonElement;
    expect(button.disabled).toBe(false);
    fireEvent.click(button);

    expect(ipc.dispatch).toHaveBeenCalledTimes(1);
    expect(ipc.dispatch).toHaveBeenCalledWith({ move_to_trash: { row: a2.key } });
  });

  it('runs the step over the whole multi-selection', () => {
    useQuickStepsStore.setState({ steps: [step('a', { action: 'archive', markRead: true })] });
    useListStore.setState({ rows: [a1, a2], selectedKeys: [a1.key, a2.key], selectedKey: a1.key });
    render(<QuickStepsRibbonGroup iconOnly={false} />);

    fireEvent.click(screen.getByRole('button', { name: 'Step a' }));

    expect(ipc.dispatch.mock.calls.map((call) => call[0])).toEqual([
      { mark_read: { row: a1.key, read: true } },
      { mark_read: { row: a2.key, read: true } },
      { archive_message: { row: a1.key } },
      { archive_message: { row: a2.key } },
    ]);
  });

  it('enables the step buttons when a selection arrives mid-render', () => {
    useQuickStepsStore.setState({ steps: [step('a')] });
    useListStore.setState({ rows: [a1] });
    render(<QuickStepsRibbonGroup iconOnly={false} />);
    expect((screen.getByRole('button', { name: 'Step a' }) as HTMLButtonElement).disabled).toBe(true);

    act(() => {
      useListStore.getState().select(a1.key);
    });
    expect((screen.getByRole('button', { name: 'Step a' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('opens the manager dialog from Manage and closes it through the gate', () => {
    render(<QuickStepsRibbonGroup iconOnly={false} />);
    expect(screen.queryByRole('dialog')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Manage quick steps' }));
    expect(useQuickStepsStore.getState().managerOpen).toBe(true);
    expect(screen.getByRole('dialog', { name: 'Quick steps' })).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(useQuickStepsStore.getState().managerOpen).toBe(false);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('collapses the labels to the tooltips under the compact breakpoint', () => {
    useQuickStepsStore.setState({ steps: [step('a', { name: 'Clean inbox' })] });
    render(<QuickStepsRibbonGroup iconOnly={true} />);

    const button = screen.getByRole('button', { name: 'Clean inbox' });
    expect(button.querySelector('span.sr-only')?.textContent).toBe('Clean inbox');
    expect(button.getAttribute('title')).toContain('Clean inbox');
  });

  it('carries the step summary on the tooltip', () => {
    useQuickStepsStore.setState({ steps: [step('a', { name: 'Clean inbox', action: 'trash', markRead: true })] });
    render(<QuickStepsRibbonGroup iconOnly={false} />);

    expect(screen.getByRole('button', { name: 'Clean inbox' }).getAttribute('title')).toBe(
      'Clean inbox: Move to Trash, mark read first',
    );
  });
});

describe('the ReadRibbon mount', () => {
  it('stands the quick steps group beside the other groups', () => {
    useQuickStepsStore.setState({ steps: [step('a', { name: 'Clean inbox' })] });
    render(<ReadRibbon iconOnly={false} />);

    expect(screen.getByRole('button', { name: 'New Email' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Clean inbox' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Manage quick steps' })).toBeTruthy();
  });

  it('hides the labels in compact mode like the other groups', () => {
    useQuickStepsStore.setState({ steps: [step('a', { name: 'Clean inbox' })] });
    render(<ReadRibbon iconOnly={true} />);

    const button = screen.getByRole('button', { name: 'Clean inbox' });
    expect(button.querySelector('span.sr-only')?.textContent).toBe('Clean inbox');
  });
});