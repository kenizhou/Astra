// SPDX-License-Identifier: GPL-3.0-only
// The message list's multi-selection model, ported from kylins' Outlook
// semantics: Ctrl toggles one row, Shift ranges from the anchor over the
// rendered items (date headers never count), Ctrl+A takes every loaded
// row, and a right-click inside the selection keeps it as the targets
// while one outside collapses the selection to the clicked row.

import { describe, expect, it } from 'vitest';

import type { ListItem } from './dateGroups';
import {
  allKeys,
  rangeKeys,
  resolveTargets,
  toggleKey,
} from './listSelection';
import type { MessageRowView, RowKey } from '@/services/surfaces';

const k1: RowKey = { message: { account: 'work', key: 'k1' } };
const k2: RowKey = { message: { account: 'work', key: 'k2' } };
const k3: RowKey = { message: { account: 'work', key: 'k3' } };
const k4: RowKey = { thread: { account: 'work', thread_id: 'th-4' } };

function row(key: RowKey): MessageRowView {
  return {
    key,
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'Subject',
    sent_at: '2026-10-09T08:30:00Z',
    unread: false,
    flagged: false,
    has_attachments: false,
  };
}

/** The rendered item list: k1, a date header, k2, k3, k4 in order. */
const items: ListItem[] = [
  { kind: 'row', row: row(k1) },
  { kind: 'header', label: 'FRIDAY' },
  { kind: 'row', row: row(k2) },
  { kind: 'row', row: row(k3) },
  { kind: 'row', row: row(k4) },
];

describe('toggleKey', () => {
  it('adds an unselected key with itself as the anchor', () => {
    const next = toggleKey([], null, k1);
    expect(next.selection).toHaveLength(1);
    expect(next.anchor).toEqual(k1);
  });

  it('removes a selected key and keeps the anchor', () => {
    const { selection, anchor } = toggleKey([k1, k2], k1, k2);
    expect(selection).toHaveLength(1);
    expect(selection[0]).toEqual(k1);
    expect(anchor).toEqual(k1);
  });

  it('drops the anchor when the anchor itself is removed', () => {
    const { selection, anchor } = toggleKey([k1, k2], k1, k1);
    expect(selection).toHaveLength(1);
    expect(anchor).toEqual(k2);
  });
});

describe('rangeKeys', () => {
  it('ranges from the anchor to the clicked key, skipping headers', () => {
    expect(rangeKeys(items, k1, k3)).toHaveLength(3);
  });

  it('ranges backwards with the same result', () => {
    expect(rangeKeys(items, k3, k1)).toHaveLength(3);
  });

  it('anchors on the clicked key when the anchor scrolled out', () => {
    const gone: RowKey = { message: { account: 'work', key: 'k99' } };
    expect(rangeKeys(items, gone, k2)).toEqual([k2]);
  });

  it('includes thread rows in a range', () => {
    expect(rangeKeys(items, k1, k4)).toHaveLength(4);
  });
});

describe('allKeys', () => {
  it('takes every row key and no header', () => {
    const keys = allKeys(items);
    expect(keys).toHaveLength(4);
    expect(keys).not.toContainEqual(expect.objectContaining({ label: 'FRIDAY' }));
  });
});

describe('resolveTargets', () => {
  it('keeps the selection when the clicked key is inside it', () => {
    const targets = resolveTargets([k1, k2, k3], k2);
    expect(targets).toHaveLength(3);
  });

  it('collapses to the clicked key outside the selection', () => {
    const targets = resolveTargets([k1, k2], k3);
    expect(targets).toEqual([k3]);
  });

  it('collapses a lone selection on its own key', () => {
    const targets = resolveTargets([k1], k1);
    expect(targets).toEqual([k1]);
  });
});
