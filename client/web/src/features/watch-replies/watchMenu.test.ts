// SPDX-License-Identifier: GPL-3.0-only
// The Watch replies entry: the label flips on the row's watched state, a
// single row toggles through the host, and a batch stands disabled with the
// reason. The service is mocked whole; the watched state rides the real store.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const service = vi.hoisted(() => ({
  watchSet: vi.fn(),
  watchClear: vi.fn(),
  watchList: vi.fn(),
}));

vi.mock('@/services/watchReplies', () => ({
  watchSet: service.watchSet,
  watchClear: service.watchClear,
  watchList: service.watchList,
}));

import { watchItem } from './watchMenu';
import { useWatchStore } from './watchStore';
import type { MessageRowView, RowKey } from '@/services/surfaces';

function row(key: RowKey): MessageRowView {
  return {
    key,
    account_id: 'work',
    sender_name: 'Ada',
    sender_address: 'ada@example.test',
    subject: 'One',
    sent_at: '2026-10-09T08:30:00Z',
    unread: false,
    flagged: false,
    has_attachments: false,
  };
}

const m1 = row({ message: { account: 'work', key: 'm1' } });
const m2 = row({ message: { account: 'work', key: 'm2' } });

beforeEach(() => {
  service.watchSet.mockReset();
  service.watchClear.mockReset();
  service.watchList.mockReset();
  service.watchList.mockResolvedValue([]);
  useWatchStore.setState({ watched: new Set(), loaded: true, revision: 0 });
});

describe('the Watch replies entry', () => {
  it('offers Watch replies on an unwatched single row and watches on select', () => {
    const item = watchItem([m1]);
    expect(item.label).toBe('Watch replies');
    expect(item.disabled).toBeUndefined();
    item.onSelect!();
    expect(service.watchSet).toHaveBeenCalledWith(m1.key);
    expect(service.watchClear).not.toHaveBeenCalled();
  });

  it('flips to Unwatch on a watched row and unwatches on select', () => {
    useWatchStore.setState({ watched: new Set(['work:m1']) });
    const item = watchItem([m1]);
    expect(item.label).toBe('Unwatch');
    item.onSelect!();
    expect(service.watchClear).toHaveBeenCalledWith(m1.key);
    expect(service.watchSet).not.toHaveBeenCalled();
  });

  it('disables a batch with the reason', () => {
    const item = watchItem([m1, m2]);
    expect(item.disabled).toBe(true);
    expect(item.title).toBe('Pick one row for this');
    expect(item.description).toBe('Pick one row for this');
    item.onSelect!();
    expect(service.watchSet).not.toHaveBeenCalled();
    expect(service.watchClear).not.toHaveBeenCalled();
  });

  it('disables an empty selection with the reason', () => {
    const item = watchItem([]);
    expect(item.disabled).toBe(true);
    expect(item.title).toBe('Pick one row for this');
  });
});