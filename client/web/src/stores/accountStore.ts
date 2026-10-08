// SPDX-License-Identifier: GPL-3.0-only
// The account list the shell keeps from mailbox-list snapshots: identities and
// labels for the status bar and the switcher, nothing more. State lives in the
// engine; this is the last snapshot's projection.

import { create } from 'zustand';

import type { MailboxListSnapshot } from '@/services/surfaces';

/** One account as the shell knows it: an identity and a label. */
export interface AccountRow {
  accountId: string;
  displayName: string;
}

interface AccountStoreState {
  /** Every configured account, in snapshot order. */
  accounts: AccountRow[];
  /** Replaces the list from a fresh mailbox-list snapshot. */
  applySnapshot: (snapshot: MailboxListSnapshot) => void;
}

export const useAccountStore = create<AccountStoreState>((set) => ({
  accounts: [],

  applySnapshot: (snapshot) =>
    set({
      accounts: snapshot.account_folders.map((account) => ({
        accountId: account.account_id,
        displayName: account.display_name,
      })),
    }),
}));
