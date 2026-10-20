// SPDX-License-Identifier: GPL-3.0-only
// The folder tree's inline rename row: the NewFolderRow pattern over an
// existing folder, prefilled with its current name, Enter committing the
// rename intent, Escape and blur cancelling, an empty name never committing.

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...ipc,
}));

import { RenameFolderRow } from './RenameFolderRow';

beforeEach(() => {
  ipc.dispatch.mockReset();
});

afterEach(cleanup);

describe('the inline rename row', () => {
  it('prefills the current name and selects it', () => {
    render(
      <RenameFolderRow account="work" folder="Projects" name="Projects" depth={0} onDone={() => undefined} />,
    );
    const input = screen.getByLabelText('Folder name') as HTMLInputElement;
    expect(input.value).toBe('Projects');
    expect(document.activeElement).toBe(input);
  });

  it('commits the rename on Enter', () => {
    const onDone = vi.fn();
    render(<RenameFolderRow account="work" folder="Projects" name="Projects" depth={0} onDone={onDone} />);
    fireEvent.change(screen.getByLabelText('Folder name'), { target: { value: 'Clients' } });
    fireEvent.keyDown(screen.getByLabelText('Folder name'), { key: 'Enter' });

    expect(ipc.dispatch).toHaveBeenCalledWith(
      expect.objectContaining({
        folders: { rename: { account: 'work', folder: 'Projects', name: 'Clients' } },
      }),
    );
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('never commits an unchanged or empty name', () => {
    const onDone = vi.fn();
    render(<RenameFolderRow account="work" folder="Projects" name="Projects" depth={0} onDone={onDone} />);
    fireEvent.keyDown(screen.getByLabelText('Folder name'), { key: 'Enter' });
    expect(ipc.dispatch).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText('Folder name'), { target: { value: '   ' } });
    fireEvent.keyDown(screen.getByLabelText('Folder name'), { key: 'Enter' });
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(onDone).not.toHaveBeenCalled();
  });

  it('cancels on Escape', () => {
    const onDone = vi.fn();
    render(<RenameFolderRow account="work" folder="Projects" name="Projects" depth={0} onDone={onDone} />);
    fireEvent.keyDown(screen.getByLabelText('Folder name'), { key: 'Escape' });
    expect(ipc.dispatch).not.toHaveBeenCalled();
    expect(onDone).toHaveBeenCalledTimes(1);
  });
});
