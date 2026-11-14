// SPDX-License-Identifier: GPL-3.0-only
// The row menu's Categorize submenu, kept out of rowMenu.ts so that file
// stays under the line cap (the blockSenderMenu pattern): a flyout of the
// local colour categories, each a checked swatch over the targets, plus the
// manager entry. The toggle is Outlook's: when EVERY target carries the
// category remove it from all, else add it to the missing. A local overlay:
// the verbs are host commands, no intent, no engine write.

import type { ContextMenuItem } from '@/components/ui/ContextMenu';
import { assignCategory, unassignCategory } from '@/services/categories';
import type { MessageRowView } from '@/services/surfaces';

import { useCategoriesStore } from './categoriesStore';

/** Whether every target carries the category: the toggle's remove leg. */
function everyTargetHas(targets: MessageRowView[], categoryId: string): boolean {
  return targets.every((row) => (row.categories ?? []).includes(categoryId));
}

/** The Outlook toggle over the targets: remove from all when every target
 *  carries the category, else add to the missing ones (the batch loops the
 *  per-row commands, the row menu's own compromise). */
export function toggleCategory(targets: MessageRowView[], categoryId: string): void {
  if (everyTargetHas(targets, categoryId)) {
    for (const row of targets) {
      void unassignCategory(row.key, categoryId);
    }
    return;
  }
  for (const row of targets) {
    if (!(row.categories ?? []).includes(categoryId)) {
      void assignCategory(row.key, categoryId);
    }
  }
}

/** The Categorize submenu over the targets: the defs as checked swatches
 *  (checked when every target carries the category), the menu staying open
 *  across picks, and the manager entry beneath. */
export function categorizeItem(targets: MessageRowView[]): ContextMenuItem {
  const defs = useCategoriesStore.getState().defs;
  const children: ContextMenuItem[] = defs.map((def) => ({
    label: def.name,
    swatch: def.color,
    checked: everyTargetHas(targets, def.id),
    keepOpenOnSelect: true,
    onSelect: () => toggleCategory(targets, def.id),
  }));
  children.push({
    label: 'Manage categories…',
    onSelect: () => useCategoriesStore.getState().open(),
  });
  return { label: 'Categorize', children };
}