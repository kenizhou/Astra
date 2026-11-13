// SPDX-License-Identifier: GPL-3.0-only
// The reminder store: open names the target message, close empties it, the
// dialog's gate the row menu's entry drives.
import { beforeEach, describe, expect, it } from 'vitest';

import { useReminderStore } from './reminderStore';

beforeEach(() => {
  useReminderStore.setState({ target: null });
});

describe('the reminder store', () => {
  it('starts closed', () => {
    expect(useReminderStore.getState().target).toBeNull();
  });

  it('open names the target message', () => {
    useReminderStore.getState().open({ account: 'work', key: 'm1' });
    expect(useReminderStore.getState().target).toEqual({ account: 'work', key: 'm1' });
  });

  it('close empties the target', () => {
    useReminderStore.getState().open({ account: 'work', key: 'm1' });
    useReminderStore.getState().close();
    expect(useReminderStore.getState().target).toBeNull();
  });
});
