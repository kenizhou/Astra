// SPDX-License-Identifier: GPL-3.0-only
// The people list: the sections render from the snapshot's own order (the
// rows arrive pre-ordered and the list never re-sorts), each row shows the
// avatar, the name (the host's own placeholder words when empty), the
// primary address, and the account disclosure, and a row tap opens the
// person.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { PersonRow } from '@/services/ipc';

import { PeopleList } from './PeopleList';

afterEach(cleanup);

function row(over: Partial<PersonRow> = {}): PersonRow {
  return {
    person: 1,
    display_name: 'Ada Lovelace',
    monogram: { initials: 'AL', colour: '#4DB6AC' },
    section: 'A',
    primary_address: 'ada@example.test',
    ...over,
  };
}

describe('the people list', () => {
  it('renders the sections in the snapshot order untouched', () => {
    const rows = [
      row({ person: 2, display_name: 'Bea Plain', section: 'B', primary_address: 'bea@example.test' }),
      row({ person: 1, display_name: 'Ada Lovelace', section: 'A' }),
      row({ person: 3, display_name: 'Øystein Nord', section: 'O', primary_address: 'oy@example.test' }),
      row({ person: 4, display_name: '3rd Party', section: '#', primary_address: 'third@example.test' }),
    ];

    render(<PeopleList rows={rows} onOpen={() => undefined} />);

    const headings = screen
      .getAllByRole('heading')
      .map((heading) => heading.textContent);
    expect(headings).toEqual(['B', 'A', 'O', '#']);
  });

  it('collapses consecutive rows of one section under the one heading', () => {
    const rows = [
      row({ person: 1, display_name: 'Ada Lovelace', section: 'A' }),
      row({ person: 2, display_name: 'Ann Other', section: 'A', primary_address: 'ann@example.test' }),
    ];

    render(<PeopleList rows={rows} onOpen={() => undefined} />);

    expect(screen.getAllByRole('heading')).toHaveLength(1);
    expect(screen.getByText('Ada Lovelace')).toBeTruthy();
    expect(screen.getByText('Ann Other')).toBeTruthy();
  });

  it('discloses the account count only where the snapshot carries it', () => {
    const rows = [
      row({ person: 1, display_name: 'Ada Lovelace', account_count: 3 }),
      row({ person: 2, display_name: 'Bea Plain' }),
    ];

    render(<PeopleList rows={rows} onOpen={() => undefined} />);

    expect(screen.getByText('In 3 accounts')).toBeTruthy();
    expect(screen.queryByText(/^In \d+ accounts$/)).not.toBeNull();
  });

  it('supplies the host placeholder words for the empty name', () => {
    render(
      <PeopleList
        rows={[row({ person: 5, display_name: '', primary_address: 'nameless@example.test' })]}
        onOpen={() => undefined}
      />,
    );

    expect(screen.getByText('No name')).toBeTruthy();
    expect(screen.getByText('nameless@example.test')).toBeTruthy();
  });

  it('opens the person whose row is tapped', () => {
    const onOpen = vi.fn();
    render(
      <PeopleList
        rows={[
          row({ person: 1, display_name: 'Ada Lovelace' }),
          row({ person: 2, display_name: 'Bea Plain', section: 'B', primary_address: 'bea@example.test' }),
        ]}
        onOpen={onOpen}
      />,
    );

    fireEvent.click(screen.getByText('Bea Plain'));

    expect(onOpen).toHaveBeenCalledWith(2);
  });
});
