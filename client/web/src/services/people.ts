// SPDX-License-Identifier: GPL-3.0-only
// The people half of the IPC wire: the PeopleSnapshot mirror (the unified
// rows with the fold order doc, the open person's detail with its sources,
// per-value provenance, and editable cards, the write states including the
// invalid one, and the create targets), the PeopleIntent family's
// constructors, the snapshot pull, the editor's card-seed read, and the
// recipient-suggestions pull. Split from ipc.ts to keep that file under
// the line cap, like the calendar half before it, and re-exported whole
// through it, so ipc stays the one import point of the wire. The literals
// are pinned in people.test.ts against the JSON astra-app's
// protocol_people_tests.rs and astra-viewmodel's people_tests.rs assert.

import { invoke } from '@tauri-apps/api/core';

/** An address book bound to its owning account; unique only within the account. */
export interface AddressBookRef {
  account: string;
  book: string;
}

/** A contact card an edit names: the person, the account, and the card together. */
export interface CardRef {
  person: number;
  account: string;
  card: string;
}

/** A contact name as the editor states it: the two fields the form holds. */
export interface ContactNameInput {
  given: string;
  surname?: string;
}

/** A new contact as the create form states it: the editor's fields. */
export interface ContactDraftInput {
  name?: ContactNameInput;
  organisation?: string;
  role?: string;
  emails?: string[];
  phones?: string[];
}

/** What an edit changes about one card: only the fields that changed. */
export interface ContactPatchInput {
  name?: ContactNameInput;
  organisation?: string;
  role?: string;
  emails?: string[];
  phones?: string[];
}

/**
 * What the people surface asks the runtime to do, nested under the `people`
 * tag of the intent that carries it. Built through the `peopleIntents`
 * constructors so the key order matches the Rust wire byte for byte.
 */
export type PeopleIntentBody =
  | 'refresh'
  | { search: { query: string } }
  | { open_person: { person: number } }
  | 'close_person'
  | { create_contact: { book: AddressBookRef; draft: ContactDraftInput } }
  | { update_contact: { card: CardRef; patch: ContactPatchInput } };

/**
 * The create constructor's optional-field discipline: a field the caller
 * did not state crosses no key at all, exactly as the Rust input's serde
 * skips do, so the literal the core reads carries nothing the form left
 * empty.
 */
function draftOf(draft: ContactDraftInput) {
  return {
    ...(draft.name !== undefined && {
      name: {
        given: draft.name.given,
        ...(draft.name.surname !== undefined && { surname: draft.name.surname }),
      },
    }),
    ...(draft.organisation !== undefined && { organisation: draft.organisation }),
    ...(draft.role !== undefined && { role: draft.role }),
    ...(draft.emails !== undefined && { emails: draft.emails }),
    ...(draft.phones !== undefined && { phones: draft.phones }),
  };
}

/** The patch constructor's discipline: the same skips over the patch fields. */
function patchOf(patch: ContactPatchInput) {
  return {
    ...(patch.name !== undefined && {
      name: {
        given: patch.name.given,
        ...(patch.name.surname !== undefined && { surname: patch.name.surname }),
      },
    }),
    ...(patch.organisation !== undefined && { organisation: patch.organisation }),
    ...(patch.role !== undefined && { role: patch.role }),
    ...(patch.emails !== undefined && { emails: patch.emails }),
    ...(patch.phones !== undefined && { phones: patch.phones }),
  };
}

/** The people constructors, spread into ipc's `intents`, the family in the enum's own order. */
export const peopleIntents = {
  /** Sync the accounts' contact sources and refresh the people snapshot. */
  refreshPeople: () => ({ people: 'refresh' as const }),
  /** Match the query in the engine and publish the matched rows as the list. */
  searchPeople: (query: string) => ({ people: { search: { query } } }),
  /** Open one person's detail, by the engine's merge-surviving person id. */
  openPerson: (person: number) => ({ people: { open_person: { person } } }),
  /** Shut the detail: the surface goes back to the list alone. */
  closePerson: () => ({ people: 'close_person' as const }),
  /** Create a contact in a writable address book of the book's own account. */
  createContact: (book: AddressBookRef, draft: ContactDraftInput) => ({
    people: { create_contact: { book, draft: draftOf(draft) } },
  }),
  /** Edit one source card, patching only the fields the edit names. */
  updateContact: (card: CardRef, patch: ContactPatchInput) => ({
    people: { update_contact: { card, patch: patchOf(patch) } },
  }),
};

/** What a person's avatar draws before any photo arrives. */
export interface Monogram {
  initials: string;
  colour: string;
}

/** One unified person, as the list renders them. */
export interface PersonRow {
  person: number;
  display_name: string;
  monogram: Monogram;
  account_count?: number;
  section: string;
  primary_address?: string;
}

/** One provider card a person was assembled from: a row of the detail's source list. */
export interface PersonSourceRow {
  account: string;
  card: string;
  writable: boolean;
}

/** The kinds of value a person's detail carries; crosses as a snake_case word. */
export type PersonValueKind = 'name' | 'email' | 'phone' | 'organisation' | 'title';

/** One of a person's values with its provenance. */
export interface PersonValueRow {
  kind: PersonValueKind;
  value: string;
  sources?: PersonSourceRow[];
}

/** One card an edit may target, carrying the person beside the account and the card. */
export interface EditableCardRow {
  person: number;
  account: string;
  card: string;
}

/** The open person's detail. */
export interface PersonDetail {
  person: number;
  display_name: string;
  sources?: PersonSourceRow[];
  values?: PersonValueRow[];
  editable_cards?: EditableCardRow[];
}

/** Where a contact write has got to; crosses as a snake_case word. */
export type PeopleWriteState = 'writing' | 'failed_with_retry' | 'invalid';

/** One account's contact write, as the surface's write status shows it. */
export interface PeopleWriteRow {
  account: string;
  state: PeopleWriteState;
}

/** One writable address book a create may land in, per account. */
export interface CreateTargetRow {
  account: string;
  book: string;
  name: string;
  is_default: boolean;
}

/**
 * The people snapshot the surface renders after every pull. The rows arrive
 * already re-ordered into the rebuild's fold (one alphabet with accented
 * letters filed under their base, one trailing `#` bucket), so the sections
 * and the list order agree; a host lays them out verbatim.
 */
export interface PeopleSnapshot {
  rows?: PersonRow[];
  detail?: PersonDetail;
  write_states?: PeopleWriteRow[];
  create_targets?: CreateTargetRow[];
}

/**
 * One ranked recipient suggestion, as the engine ordered it: the ranking is
 * the engine's (exact, prefix, substring, settled by saved-ness, recency,
 * and counts), and the client re-ranks nothing.
 */
export interface RecipientSuggestion {
  email: string;
  person_id?: number;
  display_name: string;
  provenance?: PersonSourceRow[];
  sent_count: number;
  last_sent?: string;
  is_saved: boolean;
}

/** Pulls the people snapshot. */
export function people(): Promise<PeopleSnapshot> {
  return invoke('people');
}

/**
 * Reads the stored card the editor's reference names and answers its seed,
 * the card's own values in the editor's input subset, or null when nobody
 * stores it. The editor opens on the card, never the merged detail.
 */
export function peopleCard(card: CardRef): Promise<ContactDraftInput | null> {
  return invoke('people_card', card);
}

/**
 * Pulls the ranked suggestions for one completion token: the text after the
 * last comma in the field, verbatim; splitting the field is the caller's
 * business. A blank token answers nothing.
 */
export function peopleSuggestions(token: string): Promise<RecipientSuggestion[]> {
  return invoke('people_suggestions', { token });
}
