// SPDX-License-Identifier: GPL-3.0-only
// The people surface's state: the last pulled snapshot's row sets (the
// unified rows in the fold order the snapshot carries, the open person's
// detail, the write states, the create targets). The rows arrive
// pre-ordered and pre-sectioned by the core; the store holds them verbatim
// and the list never re-sorts.

import { create } from 'zustand';

import type {
  CreateTargetRow,
  PeopleSnapshot,
  PeopleWriteRow,
  PersonDetail,
  PersonRow,
} from '@/services/ipc';

interface PeopleStoreState {
  /** The unified rows the current query matched, in the snapshot's order. */
  rows: PersonRow[];
  /** The open person's detail, or null while the list shows with nobody open. */
  detail: PersonDetail | null;
  /** The accounts with a contact write in flight, unconfirmed, or refused. */
  writeStates: PeopleWriteRow[];
  /** The writable address books a create may land in. */
  createTargets: CreateTargetRow[];
  /** Applies a pulled snapshot. */
  applySnapshot: (snapshot: PeopleSnapshot) => void;
}

export const usePeopleStore = create<PeopleStoreState>((set) => ({
  rows: [],
  detail: null,
  writeStates: [],
  createTargets: [],

  applySnapshot: (snapshot) =>
    set({
      rows: snapshot.rows ?? [],
      detail: snapshot.detail ?? null,
      writeStates: snapshot.write_states ?? [],
      createTargets: snapshot.create_targets ?? [],
    }),
}));
