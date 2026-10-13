// SPDX-License-Identifier: GPL-3.0-only
// The people list: the unified rows under their section headings, both in
// the snapshot's own order (the rows arrive pre-ordered into the fold, and
// a host that re-sorted them would disagree with the sections it renders).
// Each row shows the avatar, the name (the host's own placeholder words
// when the sources carried none), the primary address, and the account
// disclosure the snapshot carries.

import type { PersonRow } from '@/services/ipc';

import { Avatar } from './Avatar';

interface PeopleListProps {
  rows: PersonRow[];
  /** The open person's id; its row draws the selection marking. */
  selectedPerson: number | null;
  /** Opens the person whose row was tapped, by the merge-surviving id. */
  onOpen: (person: number) => void;
}

/** The host's own placeholder words for a person the sources namelessly carry. */
export const NAMELESS = 'No name';

export function PeopleList({ rows, selectedPerson, onOpen }: PeopleListProps) {
  return (
    <ul className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto p-2">
      {rows.map((row, at) => (
        <PeopleRowItem key={row.person} row={row} firstOfSection={at === 0 || rows[at - 1]?.section !== row.section} selected={row.person === selectedPerson} onOpen={onOpen} />
      ))}
    </ul>
  );
}

function PeopleRowItem({
  row,
  firstOfSection,
  selected,
  onOpen,
}: {
  row: PersonRow;
  firstOfSection: boolean;
  selected: boolean;
  onOpen: (person: number) => void;
}) {
  return (
    <>
      {firstOfSection && (
        <li>
          <h3 className="px-2 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wide text-muted-text">
            {row.section}
          </h3>
        </li>
      )}
      <li>
        <button
          type="button"
          aria-pressed={selected}
          onClick={() => onOpen(row.person)}
          className={`relative flex h-9 w-full items-center gap-2 rounded-lg px-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
            selected ? 'bg-primary-muted text-[var(--selected-text)]' : 'hover:bg-hover'
          }`}
        >
          {selected && (
            <span
              aria-hidden="true"
              className="absolute bottom-1 left-0 top-1 w-[3px] rounded-r-full iris-line-v"
            />
          )}
          <Avatar monogram={row.monogram} address={row.primary_address} size={26} />
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-[13px] font-medium leading-tight text-foreground">
              {row.display_name === '' ? NAMELESS : row.display_name}
            </span>
            {row.primary_address !== undefined && (
              <span className="truncate text-[11px] leading-tight text-muted-foreground">{row.primary_address}</span>
            )}
          </span>
          {row.account_count !== undefined && (
            <span className="shrink-0 rounded-[var(--radius-xs)] bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">
              {`In ${row.account_count} accounts`}
            </span>
          )}
        </button>
      </li>
    </>
  );
}
