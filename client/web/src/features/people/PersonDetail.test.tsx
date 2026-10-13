// SPDX-License-Identifier: GPL-3.0-only
// The person detail: the values stand in sections by kind in a fixed order,
// each value tagged with the accounts that supplied it when the person spans
// accounts, the sources named under Also in, the editable-card set driving
// the edit affordance, and a person with no editable card saying so in words.
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { PersonDetail } from '@/services/ipc';

import { PersonDetailPane } from './PersonDetail';

afterEach(cleanup);

function detail(over: Partial<PersonDetail> = {}): PersonDetail {
  return {
    person: 7,
    display_name: 'Ada Lovelace',
    sources: [
      { account: 'work', card: '/dav/work/ada.vcf', writable: true },
      { account: 'home', card: '/dav/home/ada.vcf', writable: false },
    ],
    values: [
      { kind: 'name', value: 'Ada Lovelace', sources: [] },
      {
        kind: 'email',
        value: 'ada@example.test',
        sources: [
          { account: 'work', card: '/dav/work/ada.vcf', writable: true },
          { account: 'home', card: '/dav/home/ada.vcf', writable: false },
        ],
      },
      { kind: 'phone', value: '+31 20 555 0100', sources: [{ account: 'home', card: '/dav/home/ada.vcf', writable: false }] },
    ],
    editable_cards: [{ person: 7, account: 'work', card: '/dav/work/ada.vcf' }],
    ...over,
  };
}

describe('the person detail', () => {
  it('tags each value with the accounts that supplied it', () => {
    render(<PersonDetailPane detail={detail()} onEdit={() => undefined} />);

    const email = screen.getByText('ada@example.test').closest('li');
    expect(email?.textContent).toContain('work');
    expect(email?.textContent).toContain('home');

    const phone = screen.getByText('+31 20 555 0100').closest('li');
    expect(phone?.textContent).toContain('home');
    expect(phone?.textContent).not.toContain('work');
  });

  it('names the source accounts under Also in when the person spans accounts', () => {
    render(<PersonDetailPane detail={detail()} onEdit={() => undefined} />);

    const also = screen.getByText('Also in').closest('section');
    expect(also?.textContent).toContain('work');
    expect(also?.textContent).toContain('home');
  });

  it('withholds the Also in line for a person of one account', () => {
    render(
      <PersonDetailPane
        detail={detail({
          sources: [{ account: 'work', card: '/dav/work/ada.vcf', writable: true }],
          editable_cards: [{ person: 7, account: 'work', card: '/dav/work/ada.vcf' }],
          values: [{ kind: 'email', value: 'ada@example.test', sources: [] }],
        })}
        onEdit={() => undefined}
      />,
    );

    expect(screen.queryByText('Also in')).toBeNull();
  });

  it('offers the edit affordance when an editable card stands', () => {
    const onEdit = vi.fn();
    render(<PersonDetailPane detail={detail()} onEdit={onEdit} />);

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }));

    expect(onEdit).toHaveBeenCalledTimes(1);
  });

  it('says a person with no editable card cannot be edited here', () => {
    render(
      <PersonDetailPane
        detail={detail({ editable_cards: undefined })}
        onEdit={() => undefined}
      />,
    );

    expect(screen.getByText("This contact can't be edited here")).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Edit' })).toBeNull();
  });

  it('supplies the host placeholder words for the empty name', () => {
    render(
      <PersonDetailPane
        detail={detail({ display_name: '' })}
        onEdit={() => undefined}
      />,
    );

    expect(screen.getByText('No name')).toBeTruthy();
  });

  it('sections the values by kind in the fixed order', () => {
    render(<PersonDetailPane detail={detail()} onEdit={() => undefined} />);

    const emails = screen.getByLabelText('Emails');
    const phones = screen.getByLabelText('Phones');
    expect(emails.compareDocumentPosition(phones)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    // The name kind stands in the header, never as a section.
    expect(screen.queryByLabelText('Names')).toBeNull();
  });

  it('withholds the per-value account tags for a person of one account', () => {
    render(
      <PersonDetailPane
        detail={detail({
          sources: [{ account: 'work', card: '/dav/work/ada.vcf', writable: true }],
          editable_cards: [{ person: 7, account: 'work', card: '/dav/work/ada.vcf' }],
          values: [
            {
              kind: 'email',
              value: 'ada@example.test',
              sources: [{ account: 'work', card: '/dav/work/ada.vcf', writable: true }],
            },
          ],
        })}
        onEdit={() => undefined}
      />,
    );

    expect(screen.getByText('ada@example.test').closest('li')?.textContent).not.toContain('work');
  });
});
