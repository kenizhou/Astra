// SPDX-License-Identifier: GPL-3.0-only
// The people surface's state: the last pulled snapshot's row sets (the
// unified rows in the fold order the snapshot carries, the open person's
// detail, the write states, the create targets), plus a pending create seed
// another surface (today the reading pane's sender chip) asks the people
// view to open the editor with. The rows arrive pre-ordered and
// pre-sectioned by the core; the store holds them verbatim and the list
// never re-sorts.

import { create } from 'zustand';

import type {
  CreateTargetRow,
  PeopleSnapshot,
  PeopleWriteRow,
  PersonDetail,
  PersonRow,
} from '@/services/ipc';

/** A seed another surface hands the contact editor: a name and an address. */
export interface ContactCreateSeed {
  /** The display name to prefill, empty when the source carried none. */
  name: string;
  /** The email address to prefill. */
  address: string;
}

interface PeopleStoreState {
  /** The unified rows the current query matched, in the snapshot's order. */
  rows: PersonRow[];
  /** The open person's detail, or null while the list shows with nobody open. */
  detail: PersonDetail | null;
  /** The accounts with a contact write in flight, unconfirmed, or refused. */
  writeStates: PeopleWriteRow[];
  /** The writable address books a create may land in. */
  createTargets: CreateTargetRow[];
  /** The create seed the people view opens the editor with, null while none. */
  pendingContactCreate: ContactCreateSeed | null;
  /** Applies a pulled snapshot. */
  applySnapshot: (snapshot: PeopleSnapshot) => void;
  /** Asks the people view to open the create editor prefilled. */
  requestContactCreate: (seed: ContactCreateSeed) => void;
  /** Drops the pending create seed once the editor has taken it. */
  clearContactCreate: () => void;
}

export const usePeopleStore = create<PeopleStoreState>((set) => ({
  rows: [],
  detail: null,
  writeStates: [],
  createTargets: [],
  pendingContactCreate: null,

  applySnapshot: (snapshot) =>
    set({
      rows: snapshot.rows ?? [],
      detail: snapshot.detail ?? null,
      writeStates: snapshot.write_states ?? [],
      createTargets: snapshot.create_targets ?? [],
    }),

  requestContactCreate: (seed) => set({ pendingContactCreate: seed }),

  clearContactCreate: () => set({ pendingContactCreate: null }),
}));
