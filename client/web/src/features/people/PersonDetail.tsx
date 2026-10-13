// SPDX-License-Identifier: GPL-3.0-only
// The person detail, following the mail-calendar contact detail's content
// structure: the avatar and display name up top, one labelled section per
// value kind in a fixed order, each value tagged with the accounts that
// supplied it when the person spans more than one account, the source
// accounts named under Also in for a merged person, and the editable-card
// set driving the edit affordance: a person with no editable card says so
// in words.

import type { PersonDetail, PersonValueRow, PersonValueKind } from '@/services/ipc';

import { Avatar } from './Avatar';
import { NAMELESS } from './PeopleList';

/** The value kinds with their section headings, in the detail's own order.
 *  The name kind stands in the header, never as a section. */
const SECTIONS: ReadonlyArray<{ kind: PersonValueKind; heading: string }> = [
  { kind: 'email', heading: 'Emails' },
  { kind: 'phone', heading: 'Phones' },
  { kind: 'organisation', heading: 'Organisations' },
  { kind: 'title', heading: 'Titles' },
];

interface PersonDetailPaneProps {
  detail: PersonDetail;
  /** Opens the editor over the person's editable cards. */
  onEdit: () => void;
}

/** One labelled group of values of one kind. */
function ValueSection({
  heading,
  values,
  spansSeveralAccounts,
}: {
  heading: string;
  values: PersonValueRow[];
  spansSeveralAccounts: boolean;
}) {
  if (values.length === 0) {
    return null;
  }
  return (
    <section aria-label={heading} className="flex flex-col gap-1">
      <h3 className="text-[10px] font-semibold uppercase tracking-wide text-primary">
        {heading}
      </h3>
      <ul className="flex flex-col divide-y divide-border-subtle rounded-xl border border-border-subtle bg-card">
        {values.map((value, at) => (
          <li key={`${value.kind}-${at}`} className="flex flex-col gap-0.5 px-3 py-2">
            <span className="text-[13px] text-foreground">{value.value}</span>
            {spansSeveralAccounts && (value.sources ?? []).length > 0 && (
              <span className="text-xs text-muted-foreground">
                {[...new Set(value.sources!.map((source) => source.account))].join(', ')}
              </span>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

export function PersonDetailPane({ detail, onEdit }: PersonDetailPaneProps) {
  const accounts = [...new Set((detail.sources ?? []).map((source) => source.account))];
  const spansSeveralAccounts = accounts.length > 1;
  const editable = (detail.editable_cards ?? []).length > 0;
  const values = detail.values ?? [];
  const firstEmail = values.find(
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

      {SECTIONS.map(({ kind, heading }) => (
        <ValueSection
          key={kind}
          heading={heading}
          values={values.filter((value) => value.kind === kind)}
          spansSeveralAccounts={spansSeveralAccounts}
        />
      ))}

      {spansSeveralAccounts && (
        <section aria-label="Also in" className="flex flex-col gap-1">
          <h3 className="text-[10px] font-semibold uppercase tracking-wide text-primary">
            Also in
          </h3>
          <p className="text-xs text-muted-foreground">
            {accounts.map((account) => (
              <span key={account} className="mr-2">
                {account}
              </span>
            ))}
          </p>
        </section>
      )}

      <div className="border-t border-border-subtle pt-3">
        {editable ? (
          <button
            type="button"
            onClick={onEdit}
            className="rounded-[var(--radius)] border border-border-subtle px-2.5 py-1 text-xs text-foreground transition-colors hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Edit
          </button>
        ) : (
          <p className="text-xs text-muted-foreground">This contact can't be edited here</p>
        )}
      </div>
    </div>
  );
}
