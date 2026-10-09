// SPDX-License-Identifier: GPL-3.0-only
// The compose-request store: the snapshot a pull lands, and the null that
// clears it when no request is pending.
import { beforeEach, describe, expect, it } from 'vitest';

import { useComposeRequestStore } from './composeRequestStore';
import type { ComposeRequestSnapshot } from '@/services/ipc';

/** A reply request for the fixture original, overridable per test. */
function snapshot(over: Partial<ComposeRequestSnapshot> = {}): ComposeRequestSnapshot {
  return {
    kind: 'reply',
    from: 'work',
    to: 'ada@example.test',
    cc: '',
    subject: 'Re: Quarterly report',
    in_reply_to: 'orig-1@server.test',
    document: { blocks: [], attachments: [] },
    ...over,
  };
}

beforeEach(() => {
  useComposeRequestStore.setState({ snapshot: null });
});

describe('the compose request store', () => {
  it('holds the applied snapshot and replaces it wholesale', () => {
    useComposeRequestStore.getState().applySnapshot(snapshot());
    expect(useComposeRequestStore.getState().snapshot?.kind).toBe('reply');

    useComposeRequestStore.getState().applySnapshot(
      snapshot({ kind: 'forward', to: '', in_reply_to: undefined }),
    );

    expect(useComposeRequestStore.getState().snapshot).toEqual(
      snapshot({ kind: 'forward', to: '', in_reply_to: undefined }),
    );
  });

  it('clears on null', () => {
    useComposeRequestStore.getState().applySnapshot(snapshot());

    useComposeRequestStore.getState().applySnapshot(null);

    expect(useComposeRequestStore.getState().snapshot).toBeNull();
  });
});
