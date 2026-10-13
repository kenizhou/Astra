// SPDX-License-Identifier: GPL-3.0-only
// The person detail: the person's values each tagged with the accounts that
// supplied them, the source accounts named under Also in when the person
// spans more than one, and the editable-card set driving the edit
// affordance: a person with no editable card says so in words.

import type { PersonDetail, PersonValueKind } from '@/services/ipc';

import { Avatar } from './Avatar';
import { NAMELESS } from './PeopleList';

/** The word each value kind reads as on screen. */
const KIND_WORDS: Record<PersonValueKind, string> = {
  name: 'Name',
  email: 'Email',
  phone: 'Phone',
  organisation: 'Organisation',
  title: 'Title',
};

interface PersonDetailPaneProps {
  detail: PersonDetail;
  /** Opens the editor over the person's editable cards. */
  onEdit: () => void;
}

export function PersonDetailPane({ detail, onEdit }: PersonDetailPaneProps) {
  const accounts = [...new Set((detail.sources ?? []).map((source) => source.account))];
  const editable = (detail.editable_cards ?? []).length > 0;
  const firstEmail = (detail.values ?? []).find(
    (value) => value.kind === 'email' && value.value !== '',
  )?.value;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
      <header className="flex items-center gap-3">
        <Avatar address={firstEmail} size={40} />
        <h2 className="truncate text-base font-semibold text-foreground">
          {detail.display_name === '' ? NAMELESS : detail.display_name}
        </h2>
      </header>

      {accounts.length > 1 && (
        <p className="text-xs text-muted-foreground">
          <span className="font-medium text-foreground">Also in</span>
          {accounts.map((account) => (
            <span key={account}>{` ${account}`}</span>
          ))}
        </p>
      )}

      {(detail.values ?? []).length > 0 && (
        <ul className="flex flex-col divide-y divide-border-subtle rounded-xl border border-border-subtle bg-card">
          {(detail.values ?? []).map((value, at) => (
            <li key={`${value.kind}-${at}`} className="flex flex-col gap-0.5 px-3 py-2">
              <span className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                {KIND_WORDS[value.kind]}
              </span>
              <span className="text-[13px] text-foreground">{value.value}</span>
              {(value.sources ?? []).length > 0 && (
                <span className="text-xs text-muted-foreground">
                  {[...new Set(value.sources!.map((source) => source.account))].join(', ')}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}

      {editable ? (
        <button
          type="button"
          onClick={onEdit}
          className="self-start rounded-[var(--radius)] border border-border-subtle px-2.5 py-1 text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          Edit
        </button>
      ) : (
        <p className="text-xs text-muted-foreground">This contact can't be edited here</p>
      )}
    </div>
  );
}
