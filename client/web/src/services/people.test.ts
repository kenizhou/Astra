// SPDX-License-Identifier: GPL-3.0-only
// The people wire, pinned against the literal JSON astra-app's
// protocol_people_tests.rs and astra-viewmodel's people_tests.rs assert:
// every intent crosses as exactly the bytes the Rust side reads, the
// shared fixture parses as the snapshot on both sides of the wire, and
// the three commands ask by their command names.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import type { PeopleSnapshot } from './people';
import { peopleIntents } from './people';
import fixtureJson from './__fixtures__/people_snapshot.json';

const BOOK = { account: 'work', book: '/dav/card/work/default/' };
const CARD = { person: 7, account: 'work', card: '/dav/card/work/default/ada.vcf' };

// The shared fixture, byte-for-byte the JSON astra-viewmodel's
// people_tests.rs pins for a snapshot holding every row kind (the Rust
// suite reads this same file through its own include).
describe('the people snapshot wire', () => {
  it('carries every row kind with the fold sections and the disclosure', () => {
    const snapshot = fixtureJson as PeopleSnapshot;

    expect(snapshot.rows).toEqual([
      {
        person: 7,
        display_name: 'Ada Lovelace',
        monogram: { initials: 'AL', colour: 'teal' },
        account_count: 2,
        section: 'A',
        primary_address: 'ada@example.test',
      },
    ]);
    expect(snapshot.detail?.sources).toEqual([
      { account: 'work', card: '/dav/card/work/default/ada.vcf', writable: true },
      { account: 'home', card: '/dav/card/home/default/ada.vcf', writable: false },
    ]);
    expect(snapshot.detail?.values).toEqual([
      {
        kind: 'email',
        value: 'ada@example.test',
        sources: [
          { account: 'work', card: '/dav/card/work/default/ada.vcf', writable: true },
          { account: 'home', card: '/dav/card/home/default/ada.vcf', writable: false },
        ],
      },
      {
        kind: 'phone',
        value: '+31 20 555 0100',
        sources: [{ account: 'work', card: '/dav/card/work/default/ada.vcf', writable: true }],
      },
    ]);
    expect(snapshot.detail?.editable_cards).toEqual([
      { person: 7, account: 'work', card: '/dav/card/work/default/ada.vcf' },
    ]);
    expect(snapshot.write_states).toEqual([{ account: 'work', state: 'writing' }]);
    expect(snapshot.create_targets).toEqual([
      {
        account: 'work',
        book: '/dav/card/work/default/',
        name: 'Contacts',
        is_default: true,
      },
    ]);
  });

  it('crosses an at-rest snapshot as an empty object', () => {
    const snapshot = JSON.parse('{}') as PeopleSnapshot;
    expect(snapshot.rows).toBeUndefined();
    expect(snapshot.detail).toBeUndefined();
    expect(snapshot.write_states).toBeUndefined();
    expect(snapshot.create_targets).toBeUndefined();
  });

  it('pulls the snapshot by its command name', async () => {
    invoke.mockResolvedValue({});
    const { people } = await import('./people');
    await people();
    expect(invoke).toHaveBeenCalledWith('people');
  });
});

// Every literal is byte-for-byte the JSON astra-app's
// protocol_people_tests.rs asserts, so neither side of the wire can drift.
describe('the people intent wire', () => {
  it('crosses refresh and close person as their words', () => {
    expect(JSON.stringify(peopleIntents.refreshPeople())).toBe('{"people":"refresh"}');
    expect(JSON.stringify(peopleIntents.closePerson())).toBe('{"people":"close_person"}');
  });

  it('carries the search query and the bare person id', () => {
    expect(JSON.stringify(peopleIntents.searchPeople('ada'))).toBe(
      '{"people":{"search":{"query":"ada"}}}',
    );
    expect(JSON.stringify(peopleIntents.searchPeople(''))).toBe(
      '{"people":{"search":{"query":""}}}',
    );
    expect(JSON.stringify(peopleIntents.openPerson(7))).toBe(
      '{"people":{"open_person":{"person":7}}}',
    );
  });

  it('creates a contact naming the book and skipping what is absent', () => {
    expect(
      JSON.stringify(
        peopleIntents.createContact(BOOK, {
          name: { given: 'Ada', surname: 'Lovelace' },
          organisation: 'Astra',
          role: 'Engineer',
          emails: ['ada@example.test', 'ada.lovelace@example.test'],
          phones: ['+31 20 555 0100'],
        }),
      ),
    ).toBe(
      '{"people":{"create_contact":{"book":{"account":"work","book":"/dav/card/work/default/"},"draft":{"name":{"given":"Ada","surname":"Lovelace"},"organisation":"Astra","role":"Engineer","emails":["ada@example.test","ada.lovelace@example.test"],"phones":["+31 20 555 0100"]}}}}',
    );
    expect(JSON.stringify(peopleIntents.createContact(BOOK, { organisation: 'Astra' }))).toBe(
      '{"people":{"create_contact":{"book":{"account":"work","book":"/dav/card/work/default/"},"draft":{"organisation":"Astra"}}}}',
    );
    expect(
      JSON.stringify(
        peopleIntents.createContact(BOOK, {
          name: { given: 'Ada Lovelace' },
          emails: ['ada@example.test'],
        }),
      ),
    ).toBe(
      '{"people":{"create_contact":{"book":{"account":"work","book":"/dav/card/work/default/"},"draft":{"name":{"given":"Ada Lovelace"},"emails":["ada@example.test"]}}}}',
    );
  });

  it('updates a card patching only what changed', () => {
    expect(JSON.stringify(peopleIntents.updateContact(CARD, { organisation: 'Astra' }))).toBe(
      '{"people":{"update_contact":{"card":{"person":7,"account":"work","card":"/dav/card/work/default/ada.vcf"},"patch":{"organisation":"Astra"}}}}',
    );
    expect(
      JSON.stringify(
        peopleIntents.updateContact(CARD, {
          name: { given: 'Ada', surname: 'Byron' },
          organisation: 'Astra',
          role: 'Principal Engineer',
          emails: ['ada@example.test'],
          phones: ['+31 20 555 0100', '+31 20 555 0199'],
        }),
      ),
    ).toBe(
      '{"people":{"update_contact":{"card":{"person":7,"account":"work","card":"/dav/card/work/default/ada.vcf"},"patch":{"name":{"given":"Ada","surname":"Byron"},"organisation":"Astra","role":"Principal Engineer","emails":["ada@example.test"],"phones":["+31 20 555 0100","+31 20 555 0199"]}}}}',
    );
    expect(
      JSON.stringify(
        peopleIntents.updateContact(CARD, { role: '', phones: [] }),
      ),
    ).toBe(
      '{"people":{"update_contact":{"card":{"person":7,"account":"work","card":"/dav/card/work/default/ada.vcf"},"patch":{"role":"","phones":[]}}}}',
    );
    expect(JSON.stringify(peopleIntents.updateContact(CARD, {}))).toBe(
      '{"people":{"update_contact":{"card":{"person":7,"account":"work","card":"/dav/card/work/default/ada.vcf"},"patch":{}}}}',
    );
  });
});

// The seed read and the suggestions pull ride the desktop host's own
// commands, one round trip per ask.
describe('the people command wire', () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it('asks for the card seed by its three reference parts', async () => {
    invoke.mockResolvedValue({ organisation: 'Astra' });
    const { peopleCard } = await import('./people');
    await peopleCard(CARD);
    expect(invoke).toHaveBeenCalledWith('people_card', {
      person: 7,
      account: 'work',
      card: '/dav/card/work/default/ada.vcf',
    });
  });

  it('hands the completion token through verbatim', async () => {
    invoke.mockResolvedValue([]);
    const { peopleSuggestions } = await import('./people');
    await peopleSuggestions('ad');
    expect(invoke).toHaveBeenCalledWith('people_suggestions', { token: 'ad' });
  });
});
