// SPDX-License-Identifier: GPL-3.0-only
// The people store: the last pulled snapshot's row sets, with absent sets
// landing as empty so a cold surface renders nothing rather than stale rows.
import { beforeEach, describe, expect, it } from 'vitest';

import type { PeopleSnapshot } from '@/services/ipc';

import { usePeopleStore } from './peopleStore';

beforeEach(() => {
  usePeopleStore.setState({
    rows: [],
    detail: null,
    writeStates: [],
    createTargets: [],
  });
});

describe('the people store apply', () => {
  it('lands every row set of the snapshot', () => {
    const snapshot: PeopleSnapshot = {
      rows: [
        {
          person: 7,
          display_name: 'Ada Lovelace',
          monogram: { initials: 'AL', colour: '#4DB6AC' },
          account_count: 2,
          section: 'A',
          primary_address: 'ada@example.test',
        },
      ],
      detail: {
        person: 7,
        display_name: 'Ada Lovelace',
        sources: [{ account: 'work', card: '/dav/ada.vcf', writable: true }],
        values: [{ kind: 'email', value: 'ada@example.test', sources: [] }],
        editable_cards: [{ person: 7, account: 'work', card: '/dav/ada.vcf' }],
      },
      write_states: [{ account: 'work', state: 'writing' }],
      create_targets: [
        { account: 'work', book: '/dav/book/', name: 'Contacts', is_default: true },
      ],
    };

    usePeopleStore.getState().applySnapshot(snapshot);

    const state = usePeopleStore.getState();
    expect(state.rows).toEqual(snapshot.rows);
    expect(state.detail).toEqual(snapshot.detail);
    expect(state.writeStates).toEqual(snapshot.write_states);
    expect(state.createTargets).toEqual(snapshot.create_targets);
  });

  it('lands an empty snapshot as empty sets and no detail', () => {
    usePeopleStore.getState().applySnapshot({ rows: [snapshotRow()] });

    usePeopleStore.getState().applySnapshot({});

    const state = usePeopleStore.getState();
    expect(state.rows).toEqual([]);
    expect(state.detail).toBeNull();
    expect(state.writeStates).toEqual([]);
    expect(state.createTargets).toEqual([]);
  });
});

function snapshotRow() {
  return {
    person: 1,
    display_name: 'Any One',
    monogram: { initials: 'AO', colour: '#E57373' },
    section: 'A',
  };
}
