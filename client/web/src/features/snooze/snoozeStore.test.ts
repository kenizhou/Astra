// SPDX-License-Identifier: GPL-3.0-only
// The snooze store: open names the target message, close empties it, the
// dialog's gate the row menu's entry drives.
import { beforeEach, describe, expect, it } from 'vitest';

import { useSnoozeStore } from './snoozeStore';

beforeEach(() => {
  useSnoozeStore.setState({ target: null });
});

describe('the snooze store', () => {
  it('starts closed', () => {
    expect(useSnoozeStore.getState().target).toBeNull();
  });

  it('open names the target message', () => {
    useSnoozeStore.getState().open({ account: 'work', key: 'm1' });
    expect(useSnoozeStore.getState().target).toEqual({ account: 'work', key: 'm1' });
  });

  it('close empties the target', () => {
    useSnoozeStore.getState().open({ account: 'work', key: 'm1' });
    useSnoozeStore.getState().close();
    expect(useSnoozeStore.getState().target).toBeNull();
  });
});