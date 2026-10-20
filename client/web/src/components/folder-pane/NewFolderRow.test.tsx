// SPDX-License-Identifier: GPL-3.0-only
// The folder tree's inline create row: an input standing where the new
// folder will sit, autofocused, Enter committing the create intent with
// the parent it hangs under, Escape and blur cancelling, and an empty
// name never committing.

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { NewFolderRow } from './NewFolderRow';

function renderRow(onDone = vi.fn(), depth = 1) {
  render(
    <NewFolderRow account="work" parent="Projects" depth={depth} onDone={onDone} />,
  );
  return onDone;
}

beforeEach(() => {
  ipc.dispatch.mockReset();
  // The row closes on the dispatch's settlement, so the seam always answers.
  ipc.dispatch.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe('the inline new-folder row', () => {
  it('autofocuses the input', () => {
    renderRow();
    expect(document.activeElement).toBe(screen.getByLabelText('New folder name'));
  });

  it('commits the create with its parent on Enter', async () => {
    const onDone = renderRow();
    fireEvent.change(screen.getByLabelText('New folder name'), { target: { value: 'Clients' } });
    fireEvent.keyDown(screen.getByLabelText('New folder name'), { key: 'Enter' });

    expect(ipc.dispatch).toHaveBeenCalledWith({
      folders: { create: { account: 'work', name: 'Clients', parent: 'Projects' } },
    });
    await waitFor(() => expect(onDone).toHaveBeenCalledTimes(1));
  });

  it('trims the name before committing', () => {
    renderRow();
    fireEvent.change(screen.getByLabelText('New folder name'), { target: { value: '  Clients  ' } });
    fireEvent.keyDown(screen.getByLabelText('New folder name'), { key: 'Enter' });

    expect(ipc.dispatch).toHaveBeenCalledWith({
      folders: { create: { account: 'work', name: 'Clients', parent: 'Projects' } },
    });
  });

  it('never commits an empty name', () => {
    const onDone = renderRow();
    fireEvent.keyDown(screen.getByLabelText('New folder name'), { key: 'Enter' });
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(onDone).not.toHaveBeenCalled();
  });

  it('cancels on Escape', () => {
    const onDone = renderRow();
    fireEvent.keyDown(screen.getByLabelText('New folder name'), { key: 'Escape' });
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('cancels on blur', () => {
    const onDone = renderRow();
    fireEvent.change(screen.getByLabelText('New folder name'), { target: { value: 'Clients' } });
    fireEvent.blur(screen.getByLabelText('New folder name'));
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('shows the refusal sentence and keeps the row when the host refuses', async () => {
    ipc.dispatch.mockRejectedValueOnce(new Error('that account cannot change its folders'));
    const onDone = renderRow();
    fireEvent.change(screen.getByLabelText('New folder name'), { target: { value: 'Clients' } });
    fireEvent.keyDown(screen.getByLabelText('New folder name'), { key: 'Enter' });

    expect((await screen.findByRole('alert')).textContent).toContain(
      'that account cannot change its folders',
    );
    expect(onDone).not.toHaveBeenCalled();
  });

  it('indents one level beneath its parent', () => {
    const { container } = render(
      <NewFolderRow account="work" parent="Projects" depth={2} onDone={() => undefined} />,
    );
    const row = container.firstChild as HTMLElement;
    expect(row.style.paddingLeft).toBe(`${12 + 2 * 14}px`);
  });
});
