// SPDX-License-Identifier: GPL-3.0-only
// The attachment browser's pure derivations: the sender groups the modal
// draws (newest-first, the count and total size beside each), the mime chips
// (All plus the top buckets by count, every image collapsed to Images and
// application/pdf to PDF), and the human size render the reading pane's
// attachment rows use. No React and no wire here, so the shaping is table
// tested on its own.

import type { AttachmentBrowserEntry } from '@/services/attachments';

/** How many mime buckets stand beside All; the tail folds away. */
export const MIME_CHIP_LIMIT = 6;

/** One sender's attachments: the rows newest first, the count and the summed
 *  decoded size beside the name the group header draws. */
export interface SenderGroup {
  /** The sender's bare address: the group's identity. */
  address: string;
  /** The display name, falling back to the address when none was carried. */
  name: string;
  /** The sender's rows, in the order the browse answered them. */
  entries: AttachmentBrowserEntry[];
  /** How many attachments the sender has in the browse. */
  count: number;
  /** The summed decoded byte length. */
  totalSize: number;
}

/** Groups the rows by sender address in first-appearance order: the browse
 *  answers newest first, so the first row of each sender opens its group and
 *  the groups stand newest first without a second sort. */
export function groupBySender(entries: AttachmentBrowserEntry[]): SenderGroup[] {
  const groups = new Map<string, SenderGroup>();
  for (const entry of entries) {
    const address = entry.from_address;
    let group = groups.get(address);
    if (group === undefined) {
      group = {
        address,
        name: entry.from_name !== '' ? entry.from_name : address,
        entries: [],
        count: 0,
        totalSize: 0,
      };
      groups.set(address, group);
    }
    group.entries.push(entry);
    group.count += 1;
    group.totalSize += entry.size;
  }
  return [...groups.values()];
}

/** One mime chip: the label the chip draws, the prefix a click filters by
 *  (null for All, which narrows nothing), and the count the chip carries. */
export interface MimeChip {
  label: string;
  prefix: string | null;
  count: number;
}

/** The bucket one media type falls in: every image collapses to Images,
 *  application/pdf to PDF, and any other type stands as its own bucket. */
function bucket(mediaType: string): { label: string; prefix: string } {
  const lower = mediaType.toLowerCase();
  if (lower.startsWith('image/')) {
    return { label: 'Images', prefix: 'image/' };
  }
  if (lower === 'application/pdf') {
    return { label: 'PDF', prefix: 'application/pdf' };
  }
  return { label: mediaType, prefix: mediaType };
}

/** The mime chips over a mime-unfiltered snapshot: All carrying the whole
 *  count, then the top buckets by count (ties alphabetical), capped so a long
 *  tail never crowds the row. Derived from the unfiltered rows so an active
 *  filter never collapses the chips. */
export function mimeChips(entries: AttachmentBrowserEntry[]): MimeChip[] {
  const counts = new Map<string, { label: string; prefix: string; count: number }>();
  for (const entry of entries) {
    const { label, prefix } = bucket(entry.media_type);
    const held = counts.get(prefix);
    if (held === undefined) {
      counts.set(prefix, { label, prefix, count: 1 });
    } else {
      held.count += 1;
    }
  }
  const buckets = [...counts.values()]
    .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label))
    .slice(0, MIME_CHIP_LIMIT)
    .map(({ label, prefix, count }) => ({ label, prefix, count }));
  return [{ label: 'All', prefix: null, count: entries.length }, ...buckets];
}

/** One size in bytes, rendered human: 512 B, 4.0 KB, 2.1 MB. */
export function formatSize(size: number): string {
  if (size < 1024) {
    return `${size} B`;
  }
  const kb = size / 1024;
  if (kb < 1024) {
    return `${kb.toFixed(1)} KB`;
  }
  return `${(kb / 1024).toFixed(1)} MB`;
}
