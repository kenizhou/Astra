// SPDX-License-Identifier: GPL-3.0-only
// The category manager: define a coloured category, rename or recolour it,
// delete it. A local overlay, so the copy says so up front: categories live
// on this device only, never sync, and are not mail keywords. The form offers
// a name and the twenty-swatch palette (no custom colour picker, the recorded
// gap); a delete confirms with the removed-from-every-conversation sentence
// and reports the count the cascade took. The row menu's Categorize submenu
// opens it through the categories store, the snooze dialog's own shape.

import { useState } from 'react';

import { useDialogFocus } from '@/components/ui/useDialogFocus';
import {
  CATEGORY_PALETTE,
  createCategory,
  deleteCategory,
  updateCategory,
  type CategoryDef,
} from '@/services/categories';
import { useListFilterStore } from '@/stores/listFilterStore';

import { useCategoriesStore } from './categoriesStore';

/** The store gate: the dialog stands while the manager is open. */
export function CategoriesDialog() {
  const open = useCategoriesStore((s) => s.dialogOpen);
  if (!open) {
    return null;
  }
  return <CategoriesDialogBody />;
}

/** The modal over the definitions and the create/edit form. */
function CategoriesDialogBody() {
  const dialogRef = useDialogFocus<HTMLDivElement>();
  const defs = useCategoriesStore((s) => s.defs);
  const [name, setName] = useState('');
  const [color, setColor] = useState(CATEGORY_PALETTE[0]!);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<CategoryDef | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [removedNote, setRemovedNote] = useState<string | null>(null);

  const close = (): void => useCategoriesStore.getState().close();
  const reload = (): Promise<void> => useCategoriesStore.getState().loadCategories();

  const resetForm = (): void => {
    setName('');
    setColor(CATEGORY_PALETTE[0]!);
    setEditingId(null);
    setError(null);
  };

  const submit = async (): Promise<void> => {
    setError(null);
    setRemovedNote(null);
    try {
      if (editingId === null) {
        await createCategory(name, color);
      } else {
        await updateCategory(editingId, name, color);
      }
      await reload();
      resetForm();
    } catch (err) {
      setError(String(err));
    }
  };

  const startEdit = (def: CategoryDef): void => {
    setEditingId(def.id);
    setName(def.name);
    setColor(def.color);
    setError(null);
    setRemovedNote(null);
    setConfirmDelete(null);
  };

  const doDelete = async (): Promise<void> => {
    if (confirmDelete === null) {
      return;
    }
    const id = confirmDelete.id;
    setConfirmDelete(null);
    try {
      const removed = await deleteCategory(id);
      // A filter naming the gone category clears with it: the picker never
      // narrows to a definition that no longer stands.
      const filterStore = useListFilterStore.getState();
      if (filterStore.categoryFilter === id) {
        filterStore.setCategoryFilter(null);
      }
      await reload();
      setRemovedNote(
        `Removed from ${removed} conversation${removed === 1 ? '' : 's'}.`,
      );
      if (editingId === id) {
        resetForm();
      }
    } catch (err) {
      setError(String(err));
    }
  };

  const field =
    'rounded-[var(--radius)] border border-border-subtle bg-background px-2.5 py-1 text-sm text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]';
  const footer =
    'rounded-[var(--radius)] px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring min-h-[var(--hit)]';
  const small =
    'rounded-[var(--radius-xs)] px-1.5 py-0.5 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring';

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
        aria-label="Manage categories"
        ref={dialogRef}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            close();
          }
        }}
        className="flex max-h-full w-[460px] flex-col overflow-y-auto rounded-xl border border-border-subtle bg-card p-5 shadow-[var(--shadow-lg)] focus-visible:outline-none"
      >
        <h2 className="mb-2 text-sm font-semibold text-foreground">Categories</h2>
        <p className="text-xs text-muted-text">
          Categories are local to this device. They do not sync, are not shared with
          other devices, and are not mail keywords.
        </p>

        {defs.length > 0 && (
          <ul aria-label="Category definitions" className="mt-3 flex flex-col gap-1">
            {defs.map((def) => (
              <li
                key={def.id}
                className="flex items-center gap-2 rounded-[var(--radius)] border border-border-subtle px-2 py-1"
              >
                <span
                  aria-hidden="true"
                  className="h-3 w-3 shrink-0 rounded-full"
                  style={{ backgroundColor: def.color }}
                />
                <span className="min-w-0 flex-1 truncate text-sm text-foreground">{def.name}</span>
                <button type="button" className={small} onClick={() => startEdit(def)}>
                  Edit
                </button>
                <button
                  type="button"
                  className={small}
                  onClick={() => {
                    setConfirmDelete(def);
                    setRemovedNote(null);
                  }}
                >
                  Delete
                </button>
              </li>
            ))}
          </ul>
        )}

        {confirmDelete !== null && (
          <div
            role="alertdialog"
            aria-label="Confirm delete"
            className="mt-3 rounded-[var(--radius)] border border-border-subtle bg-muted/40 p-3"
          >
            <p className="text-sm text-foreground">
              Delete “{confirmDelete.name}”? This removes it from every conversation.
            </p>
            <div className="mt-2 flex justify-end gap-2">
              <button
                type="button"
                className={`${footer} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
                onClick={() => setConfirmDelete(null)}
              >
                Cancel
              </button>
              <button
                type="button"
                className={`${footer} bg-destructive text-destructive-foreground hover:opacity-90`}
                onClick={() => void doDelete()}
              >
                Delete
              </button>
            </div>
          </div>
        )}

        {removedNote !== null && (
          <p role="status" className="mt-2 text-xs text-muted-text">
            {removedNote}
          </p>
        )}

        <div className="mt-4 border-t border-border-subtle pt-3">
          <h3 className="mb-2 text-xs font-semibold text-foreground">
            {editingId === null ? 'New category' : 'Edit category'}
          </h3>
          <label className="flex flex-col gap-1">
            <span className="text-xs text-muted-text">Name</span>
            <input
              type="text"
              value={name}
              maxLength={64}
              onChange={(event) => setName(event.target.value)}
              className={field}
              placeholder="e.g. Work"
            />
          </label>
          <div role="radiogroup" aria-label="Colour" className="mt-3 flex flex-wrap gap-1.5">
            {CATEGORY_PALETTE.map((swatch) => (
              <button
                key={swatch}
                type="button"
                role="radio"
                aria-checked={color === swatch}
                aria-label={swatch}
                title={swatch}
                onClick={() => setColor(swatch)}
                className={`h-6 w-6 rounded-full border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                  color === swatch ? 'border-foreground ring-2 ring-ring' : 'border-border-subtle'
                }`}
                style={{ backgroundColor: swatch }}
              />
            ))}
          </div>
          {error !== null && (
            <p role="alert" className="mt-2 text-xs text-destructive">
              {error}
            </p>
          )}
          <div className="mt-3 flex justify-end gap-2">
            {editingId !== null && (
              <button
                type="button"
                className={`${footer} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
                onClick={resetForm}
              >
                Cancel edit
              </button>
            )}
            <button
              type="button"
              disabled={name.trim() === ''}
              className={`${footer} bg-primary text-primary-foreground hover:opacity-90 disabled:cursor-default disabled:opacity-50`}
              onClick={() => void submit()}
            >
              {editingId === null ? 'Create category' : 'Save changes'}
            </button>
          </div>
        </div>

        <div className="mt-4 flex justify-end border-t border-border-subtle pt-3">
          <button
            type="button"
            onClick={close}
            className={`${footer} border border-border-subtle text-muted-foreground hover:bg-hover hover:text-foreground`}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}