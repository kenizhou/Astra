// SPDX-License-Identifier: GPL-3.0-only
// The account list the shell keeps from mailbox-list snapshots: each row
// carries the sending address beside the label, the field a From picker
// offers and the status bar can show.
import { describe, expect, it } from 'vitest';

import type { MailboxListSnapshot } from '@/services/surfaces';

import { useAccountStore } from './accountStore';

/** A two-account snapshot whose rows carry their sending addresses. */
function snapshot(): MailboxListSnapshot {
  return {
    account_folders: [
      {
        account_id: 'work',
        display_name: 'Work',
        address: 'ada@work.test',
        expanded: true,
        folders: [],
      },
      {
        account_id: 'home',
        display_name: 'home@personal.test',
        address: 'home@personal.test',
        expanded: false,
        folders: [],
      },
    ],
  };
}

describe('the account store', () => {
  it("keeps each account row's sending address beside its label", () => {
    useAccountStore.getState().applySnapshot(snapshot());

    expect(useAccountStore.getState().accounts).toEqual([
      { accountId: 'work', displayName: 'Work', address: 'ada@work.test' },
      { accountId: 'home', displayName: 'home@personal.test', address: 'home@personal.test' },
    ]);
  });
});
