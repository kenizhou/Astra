// SPDX-License-Identifier: GPL-3.0-only
// The attachment browser's pure derivations: the sender groups (newest-first,
// count and total size beside each), the mime chips (All plus the top buckets
// by count, image collapsed to Images and pdf to PDF, derived from a
// mime-unfiltered snapshot), and the human size render. Table tests over plain
// rows; no React and no wire here.

import { describe, expect, it } from 'vitest';

import type { AttachmentBrowserEntry } from '@/services/attachments';

import { formatSize, groupBySender, mimeChips } from './grouping';

/** One browsed row, overridable per test. */
function entry(over: Partial<AttachmentBrowserEntry> = {}): AttachmentBrowserEntry {
  return {
    id: 1,
    file_name: 'report.pdf',
    media_type: 'application/pdf',
    size: 1024,
    source: { account: 'work', key: 'm1' },
    subject: 'Launch',
    from_name: 'Ada Lovelace',
    from_address: 'ada@example.test',
    sent_at: '2026-10-09T08:00:00Z',
    ...over,
  };
}

describe('groupBySender', () => {
  it('returns no groups for no rows', () => {
    expect(groupBySender([])).toEqual([]);
  });

  it('groups rows by sender address, newest group first', () => {
    const rows = [
      entry({ source: { account: 'work', key: 'm2' }, from_address: 'grace@example.test', from_name: 'Grace Hopper', sent_at: '2026-10-09T09:30:00Z' }),
      entry({ source: { account: 'work', key: 'm1' }, sent_at: '2026-10-09T08:00:00Z' }),
    ];
    const groups = groupBySender(rows);
    expect(groups.map((group) => group.address)).toEqual(['grace@example.test', 'ada@example.test']);
  });

  it('carries the count and total size beside each group', () => {
    const rows = [
      entry({ id: 1, size: 1024 }),
      entry({ id: 2, file_name: 'logo.png', media_type: 'image/png', size: 2048 }),
    ];
    const groups = groupBySender(rows);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.count).toBe(2);
    expect(groups[0]!.totalSize).toBe(3072);
    expect(groups[0]!.entries.map((row) => row.id)).toEqual([1, 2]);
  });

  it('names the group by the display name, falling back to the address', () => {
    const named = groupBySender([entry()]);
    expect(named[0]!.name).toBe('Ada Lovelace');
    const bare = groupBySender([entry({ from_name: '' })]);
    expect(bare[0]!.name).toBe('ada@example.test');
  });

  it('keeps a message parts together under one sender across messages', () => {
    const rows = [
      entry({ source: { account: 'work', key: 'm2' }, sent_at: '2026-10-09T09:00:00Z' }),
      entry({ source: { account: 'work', key: 'm1' }, sent_at: '2026-10-09T08:00:00Z' }),
    ];
    const groups = groupBySender(rows);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.count).toBe(2);
  });

  it('keeps the browse order, undated groups last as the core answers them', () => {
    // The core answers newest first with undated rows last; the grouping
    // preserves that order rather than re-sorting it.
    const rows = [
      entry({ source: { account: 'work', key: 'm2' }, from_address: 'grace@example.test', sent_at: '2026-10-09T09:00:00Z' }),
      entry({ source: { account: 'work', key: 'm1' }, sent_at: '' }),
    ];
    const groups = groupBySender(rows);
    expect(groups.map((group) => group.address)).toEqual(['grace@example.test', 'ada@example.test']);
  });
});

describe('mimeChips', () => {
  it('offers only All for no rows', () => {
    expect(mimeChips([])).toEqual([{ label: 'All', prefix: null, count: 0 }]);
  });

  it('leads with All carrying the whole count', () => {
    const chips = mimeChips([entry(), entry({ id: 2, media_type: 'image/png' })]);
    expect(chips[0]).toEqual({ label: 'All', prefix: null, count: 2 });
  });

  it('collapses every image type to one Images bucket', () => {
    const rows = [
      entry({ media_type: 'image/png' }),
      entry({ id: 2, media_type: 'image/jpeg' }),
      entry({ id: 3, media_type: 'application/pdf' }),
    ];
    const chips = mimeChips(rows);
    const images = chips.find((chip) => chip.label === 'Images');
    expect(images).toEqual({ label: 'Images', prefix: 'image/', count: 2 });
  });

  it('names application/pdf as PDF', () => {
    const chips = mimeChips([entry(), entry({ id: 2, media_type: 'image/png' })]);
    const pdf = chips.find((chip) => chip.label === 'PDF');
    expect(pdf).toEqual({ label: 'PDF', prefix: 'application/pdf', count: 1 });
  });

  it('orders the buckets by count, descending', () => {
    const rows = [
      entry({ media_type: 'application/pdf' }),
      entry({ id: 2, media_type: 'image/png' }),
      entry({ id: 3, media_type: 'image/jpeg' }),
    ];
    const chips = mimeChips(rows);
    expect(chips.map((chip) => chip.label)).toEqual(['All', 'Images', 'PDF']);
  });

  it('buckets an uncollapsed type by its own media type', () => {
    const chips = mimeChips([entry({ media_type: 'text/plain' })]);
    expect(chips[1]).toEqual({ label: 'text/plain', prefix: 'text/plain', count: 1 });
  });

  it('caps the buckets beside All', () => {
    const rows = Array.from({ length: 10 }, (_, i) =>
      entry({ id: i, media_type: `application/x-type-${i}` }),
    );
    const chips = mimeChips(rows);
    // All plus the cap of buckets, never the whole tail.
    expect(chips.length).toBeLessThanOrEqual(7);
    expect(chips[0]!.label).toBe('All');
  });
});

describe('formatSize', () => {
  it('renders bytes, kilobytes, and megabytes', () => {
    expect(formatSize(512)).toBe('512 B');
    expect(formatSize(4096)).toBe('4.0 KB');
    expect(formatSize(2 * 1024 * 1024)).toBe('2.0 MB');
  });
});
