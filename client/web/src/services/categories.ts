// SPDX-License-Identifier: GPL-3.0-only
// The category half of the IPC wire: the manager's CRUD and the row menu's
// Categorize verbs. Split from ipc.ts to keep that file under the line cap
// (the wire types ride here, never there), like the snooze half before it.
// Categories are host commands, not intents, and a LOCAL overlay: no engine
// write, no sync, no protocol concept. The row wire carries ids only; the
// names and colours resolve surface-side from the definitions snapshot.

import { invoke } from '@tauri-apps/api/core';

import type { RowRef } from './ipc';

/** One category definition as the host carries it: the slug id, the display
 *  name, and the `#RRGGBB` colour. */
export interface CategoryDef {
  /** The definition's slug id, the key the assignments and the row wire use. */
  id: string;
  /** The display name, unique case-insensitively. */
  name: string;
  /** The colour, a `#RRGGBB` hex spelling. */
  color: string;
}

/** The palette the manager offers: twenty fixed `#RRGGBB` swatches. The
 *  backend accepts any `#RRGGBB`; the manager offers only these (no custom
 *  colour picker, the recorded gap). */
export const CATEGORY_PALETTE: readonly string[] = [
  '#E53935',
  '#D81B60',
  '#8E24AA',
  '#5E35B1',
  '#3949AB',
  '#1E88E5',
  '#039BE5',
  '#00ACC1',
  '#00897B',
  '#43A047',
  '#7CB342',
  '#C0CA33',
  '#FDD835',
  '#FFB300',
  '#FB8C00',
  '#F4511E',
  '#6D4C41',
  '#757575',
  '#546E7A',
  '#263238',
];

/** Every definition, creation order; an app with no store answers empty. */
export function listCategories(): Promise<CategoryDef[]> {
  return invoke('categories_list');
}

/** Creates a definition, answering the landed row. */
export function createCategory(name: string, color: string): Promise<CategoryDef> {
  return invoke('categories_create', { name, color });
}

/** Renames and recolours a definition in place; the assignments survive. */
export function updateCategory(id: string, name: string, color: string): Promise<void> {
  return invoke('categories_update', { id, name, color });
}

/** Deletes a definition and its assignments, answering the removed count. */
export function deleteCategory(id: string): Promise<number> {
  return invoke('categories_delete', { id });
}

/** Assigns a category to the row's conversation, answering whether this call
 *  inserted (idempotent). */
export function assignCategory(row: RowRef, categoryId: string): Promise<boolean> {
  return invoke('categories_assign', { row, categoryId });
}

/** Removes a category from the row's conversation, answering whether a row
 *  was removed (idempotent). */
export function unassignCategory(row: RowRef, categoryId: string): Promise<boolean> {
  return invoke('categories_unassign', { row, categoryId });
}