// SPDX-License-Identifier: GPL-3.0-only
// The category manager: the local-only copy up front, the create form over
// the palette, the rename/recolour edit, and the delete confirm with the
// removed-from-every-conversation sentence and its count. The service is
// mocked whole; the defs ride the real store.
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const service = vi.hoisted(() => ({
  listCategories: vi.fn(),
  createCategory: vi.fn(),
  updateCategory: vi.fn(),
  deleteCategory: vi.fn(),
}));

vi.mock('@/services/categories', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ...service,
}));

import { CategoriesDialog } from './CategoriesDialog';
import { useCategoriesStore } from './categoriesStore';
import { useListFilterStore } from '@/stores/listFilterStore';

const defs = [
  { id: 'work', name: 'Work', color: '#FF0000' },
  { id: 'personal', name: 'Personal', color: '#00FF00' },
];

beforeEach(() => {
  service.listCategories.mockReset();
  service.createCategory.mockReset();
  service.updateCategory.mockReset();
  service.deleteCategory.mockReset();
  service.listCategories.mockResolvedValue(defs);
  useCategoriesStore.setState({ defs, revision: 0, loaded: true, dialogOpen: true });
  useListFilterStore.setState({ filter: 'all', categoryFilter: null });
});

afterEach(() => {
  cleanup();
  useCategoriesStore.setState({ dialogOpen: false });
});

describe('the category manager', () => {
  it('says the categories are local to this device up front', () => {
    render(<CategoriesDialog />);
    expect(screen.getByText(/local to this device/i)).toBeTruthy();
  });

  it('renders nothing while the gate is closed', () => {
    useCategoriesStore.setState({ dialogOpen: false });
    const { container } = render(<CategoriesDialog />);
    expect(container.firstChild).toBeNull();
  });

  it('creates a category from the name and the chosen palette colour', async () => {
    service.createCategory.mockResolvedValue({ id: 'travel', name: 'Travel', color: '#1E88E5' });
    render(<CategoriesDialog />);

    fireEvent.change(screen.getByPlaceholderText('e.g. Work'), { target: { value: 'Travel' } });
    fireEvent.click(screen.getByLabelText('#1E88E5'));
    await act(async () => {
      fireEvent.click(screen.getByText('Create category'));
    });

    expect(service.createCategory).toHaveBeenCalledWith('Travel', '#1E88E5');
  });

  it('refuses an empty name with the create button disabled', () => {
    render(<CategoriesDialog />);
    expect((screen.getByText('Create category') as HTMLButtonElement).disabled).toBe(true);
  });

  it('loads a definition into the form on edit and saves the rename', async () => {
    service.updateCategory.mockResolvedValue(undefined);
    render(<CategoriesDialog />);

    fireEvent.click(screen.getAllByText('Edit')[0]!);
    const input = screen.getByPlaceholderText('e.g. Work') as HTMLInputElement;
    expect(input.value).toBe('Work');
    fireEvent.change(input, { target: { value: 'Projects' } });
    await act(async () => {
      fireEvent.click(screen.getByText('Save changes'));
    });

    expect(service.updateCategory).toHaveBeenCalledWith('work', 'Projects', '#FF0000');
  });

  it('confirms a delete with the removed-from-every-conversation copy and reports the count', async () => {
    service.deleteCategory.mockResolvedValue(3);
    render(<CategoriesDialog />);

    fireEvent.click(screen.getAllByText('Delete')[0]!);
    expect(screen.getByText(/removes it from every conversation/i)).toBeTruthy();
    await act(async () => {
      fireEvent.click(screen.getByRole('alertdialog').querySelector('button.bg-destructive')!);
    });

    expect(service.deleteCategory).toHaveBeenCalledWith('work');
    await waitFor(() => expect(screen.getByText('Removed from 3 conversations.')).toBeTruthy());
  });

  it('clears a category filter naming the deleted definition', async () => {
    service.deleteCategory.mockResolvedValue(1);
    useListFilterStore.setState({ categoryFilter: 'work' });
    render(<CategoriesDialog />);

    fireEvent.click(screen.getAllByText('Delete')[0]!);
    await act(async () => {
      fireEvent.click(screen.getByRole('alertdialog').querySelector('button.bg-destructive')!);
    });

    await waitFor(() => expect(useListFilterStore.getState().categoryFilter).toBeNull());
  });
});