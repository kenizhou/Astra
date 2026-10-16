// SPDX-License-Identifier: GPL-3.0-only
// The message list's multi-selection model, ported from kylins' Outlook
// semantics: Ctrl toggles one row, Shift ranges from the anchor over the
// rendered items (date headers never count), Ctrl+A takes every loaded
// row, and a right-click inside the selection keeps it as the targets
// while one outside collapses the selection to the clicked row. Pure
// functions over the rendered items; the list store holds the state.

import type { ListItem } from './dateGroups';
import { sameRowKey } from './rowIdentity';
import type { RowKey } from '@/services/surfaces';

/** The row keys one item list carries, in render order, headers skipped. */
function rowKeys(items: ListItem[]): RowKey[] {
  return items.filter((item) => item.kind === 'row').map((item) => item.row.key);
}

/** Ctrl: one key in or out of the selection; the anchor follows the add. */
export function toggleKey(
  selection: RowKey[],
  anchor: RowKey | null,
  key: RowKey,
): { selection: RowKey[]; anchor: RowKey | null } {
  if (selection.some((k) => sameRowKey(k, key))) {
    const next = selection.filter((k) => !sameRowKey(k, key));
    return {
      selection: next,
      anchor: anchor !== null && sameRowKey(anchor, key) ? (next[0] ?? null) : anchor,
    };
  }
  return { selection: [...selection, key], anchor: key };
}

/** Shift: the contiguous run from the anchor to the clicked key. */
export function rangeKeys(items: ListItem[], anchor: RowKey | null, key: RowKey): RowKey[] {
  const keys = rowKeys(items);
  const to = keys.findIndex((k) => sameRowKey(k, key));
  if (to < 0) {
    return [key];
  }
  const from = anchor === null ? -1 : keys.findIndex((k) => sameRowKey(k, anchor));
  if (from < 0) {
    return [key];
  }
  const [lo, hi] = from < to ? [from, to] : [to, from];
  return keys.slice(lo, hi + 1);
}

/** Ctrl+A: every loaded row, headers excluded. */
export function allKeys(items: ListItem[]): RowKey[] {
  return rowKeys(items);
}

/**
 * The rows one menu action applies to: the clicked key's selection when it
 * rides one, the clicked key alone otherwise (the selection collapses to
 * it, which the caller records).
 */
export function resolveTargets(selection: RowKey[], clicked: RowKey): RowKey[] {
  return selection.length > 1 && selection.some((k) => sameRowKey(k, clicked))
    ? selection
    : [clicked];
}
