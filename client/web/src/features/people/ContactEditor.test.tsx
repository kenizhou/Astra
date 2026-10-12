// SPDX-License-Identifier: GPL-3.0-only
// The contact editor: create offers the writable books and dispatches the
// create with the chosen one, an edit asks which card when several stand
// (naming the account) and opens straight when one does, the form seeds from
// the card's own values (never the merged detail), an edit that changed
// nothing offers no save, and the Invalid sentence renders under the form.
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const ipc = vi.hoisted(() => ({ dispatch: vi.fn(), peopleCard: vi.fn() }));

vi.mock('@/services/ipc', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  dispatch: ipc.dispatch,
  peopleCard: ipc.peopleCard,
}));

import type { CreateTargetRow, EditableCardRow, PeopleWriteRow } from '@/services/ipc';

import { ContactEditor } from './ContactEditor';

const TARGETS: CreateTargetRow[] = [
  { account: 'work', book: '/dav/work/book/', name: 'Work contacts', is_default: false },
  { account: 'home', book: '/dav/home/book/', name: 'Personal', is_default: true },
];

const CARDS: EditableCardRow[] = [
  { person: 7, account: 'work', card: '/dav/work/ada.vcf' },
  { person: 7, account: 'home', card: '/dav/home/ada.vcf' },
];

const SEED = {
  name: { given: 'Ada', surname: 'Lovelace' },
  organisation: 'Ministry of Compute',
  emails: ['ada@example.test'],
  phones: [],
};

beforeEach(() => {
  ipc.dispatch.mockReset();
  ipc.dispatch.mockResolvedValue(undefined);
  ipc.peopleCard.mockReset();
  ipc.peopleCard.mockResolvedValue(SEED);
});

afterEach(cleanup);

function writes(states: PeopleWriteRow[]): PeopleWriteRow[] {
  return states;
}

describe('the contact editor', () => {
  it('offers the writable books and dispatches the create onto the chosen one', async () => {
    render(
      <ContactEditor mode="create" createTargets={TARGETS} editableCards={[]} writeStates={[]} onClose={() => undefined} />,
    );

    const book = await screen.findByLabelText('Address book');
    expect((book as HTMLSelectElement).value).toBe('home:/dav/home/book/');

    fireEvent.change(screen.getByLabelText('Given name'), { target: { value: 'Bea' } });
    fireEvent.change(screen.getByLabelText('Surname'), { target: { value: 'Plain' } });
    fireEvent.change(screen.getByLabelText('Emails'), { target: { value: 'bea@example.test' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"people":{"create_contact":{"book":{"account":"home","book":"/dav/home/book/"},' +
        '"draft":{"name":{"given":"Bea","surname":"Plain"},"emails":["bea@example.test"]}}}}',
    );
  });

  it('asks which card an edit targets when several stand, naming the accounts', async () => {
    render(
      <ContactEditor mode="edit" createTargets={[]} editableCards={CARDS} writeStates={[]} onClose={() => undefined} />,
    );

    expect(screen.getByRole('dialog', { name: 'Edit contact' })).toBeTruthy();
    expect(screen.getByLabelText('Edit in work')).toBeTruthy();
    expect(screen.getByLabelText('Edit in home')).toBeTruthy();
    expect(screen.queryByLabelText('Given name')).toBeNull();
    expect(ipc.peopleCard).not.toHaveBeenCalled();

    fireEvent.click(screen.getByLabelText('Edit in home'));
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }));

    await waitFor(() => {
      expect(ipc.peopleCard).toHaveBeenCalledWith({
        person: 7,
        account: 'home',
        card: '/dav/home/ada.vcf',
      });
    });
  });

  it('opens straight onto the one card and seeds the form from the card', async () => {
    render(
      <ContactEditor
        mode="edit"
        createTargets={[]}
        editableCards={[CARDS[0]!]}
        writeStates={[]}
        onClose={() => undefined}
      />,
    );

    expect(await screen.findByLabelText('Given name')).toBeTruthy();
    expect((screen.getByLabelText('Given name') as HTMLInputElement).value).toBe('Ada');
    expect((screen.getByLabelText('Surname') as HTMLInputElement).value).toBe('Lovelace');
    expect((screen.getByLabelText('Organisation') as HTMLInputElement).value).toBe(
      'Ministry of Compute',
    );
    expect((screen.getByLabelText('Emails') as HTMLInputElement).value).toBe('ada@example.test');
    expect(ipc.peopleCard).toHaveBeenCalledWith({
      person: 7,
      account: 'work',
      card: '/dav/work/ada.vcf',
    });
  });

  it('offers no save while an edit changed nothing', async () => {
    render(
      <ContactEditor
        mode="edit"
        createTargets={[]}
        editableCards={[CARDS[0]!]}
        writeStates={[]}
        onClose={() => undefined}
      />,
    );

    await screen.findByLabelText('Given name');
    const save = screen.getByRole('button', { name: 'Save' }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);

    fireEvent.change(screen.getByLabelText('Given name'), { target: { value: 'Ada2' } });
    expect(save.disabled).toBe(false);
  });

  it('sends only the fields an edit changed', async () => {
    render(
      <ContactEditor
        mode="edit"
        createTargets={[]}
        editableCards={[CARDS[0]!]}
        writeStates={[]}
        onClose={() => undefined}
      />,
    );

    await screen.findByLabelText('Given name');
    fireEvent.change(screen.getByLabelText('Organisation'), { target: { value: 'Analytical Engines' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(JSON.stringify(ipc.dispatch.mock.calls[0]?.[0])).toBe(
      '{"people":{"update_contact":{"card":{"person":7,"account":"work","card":"/dav/work/ada.vcf"},' +
        '"patch":{"organisation":"Analytical Engines"}}}}',
    );
  });

  it('renders the invalid sentence under the form', async () => {
    render(
      <ContactEditor
        mode="edit"
        createTargets={[]}
        editableCards={[CARDS[0]!]}
        writeStates={writes([{ account: 'work', state: 'invalid' }])}
        onClose={() => undefined}
      />,
    );

    await screen.findByLabelText('Given name');
    const sentence = screen.getByText('The contact could not be saved. Check the fields, then try again.');
    const form = screen.getByLabelText('Given name').closest('form');
    expect(form).not.toBeNull();
    expect(form?.contains(sentence)).toBe(true);
  });
});
