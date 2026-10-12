// SPDX-License-Identifier: GPL-3.0-only
// The contact editor: a create over the writable books (the book picker,
// preselected on the default), or an edit of one source card, asking which
// card when several stand (naming the account) and opening straight when
// one does. The form seeds from the card's own values through the wire's
// seed read, never the merged detail; a patch names only the fields the
// edit changed, and an edit that changed nothing offers no save at all. A
// refused (Invalid) write is what keeps the form open: the close waits for
// the write's settle through the subscribed write states, and the Invalid
// sentence renders under the form the user is still looking at, for the
// account the form writes to and no other.

import { useEffect, useState } from 'react';

import {
  dispatch,
  intents,
  peopleCard,
  type ContactDraftInput,
  type ContactPatchInput,
  type CreateTargetRow,
  type EditableCardRow,
  type PeopleWriteRow,
} from '@/services/ipc';

/** The sentence a refused form draws, under itself. */
const INVALID_SENTENCE =
  'The contact could not be saved. Check the fields, then try again.';

interface ContactEditorProps {
  mode: 'create' | 'edit';
  createTargets: CreateTargetRow[];
  editableCards: EditableCardRow[];
  writeStates: PeopleWriteRow[];
  onClose: () => void;
}

/** The editor's fields as one flat state: the two name parts, the two flat
 * words, and the two lists as comma-separated text. */
interface EditorForm {
  given: string;
  surname: string;
  organisation: string;
  role: string;
  emails: string;
  phones: string;
}

const EMPTY_FORM: EditorForm = {
  given: '',
  surname: '',
  organisation: '',
  role: '',
  emails: '',
  phones: '',
};

/** One seed rendered as the form holds it: absent list fields join to the
 * empty string so a comparison against them is a comparison of like kinds. */
function formOfSeed(seed: ContactDraftInput | null): EditorForm {
  return {
    given: seed?.name?.given ?? '',
    surname: seed?.name?.surname ?? '',
    organisation: seed?.organisation ?? '',
    role: seed?.role ?? '',
    emails: (seed?.emails ?? []).join(', '),
    phones: (seed?.phones ?? []).join(', '),
  };
}

/** Splits one comma-separated list field into its values, blanks dropped. */
function listOf(text: string): string[] {
  return text
    .split(',')
    .map((value) => value.trim())
    .filter((value) => value !== '');
}

const fieldClass =
  'h-7 w-full rounded-[var(--radius-xs)] border border-input bg-background px-2 text-xs text-foreground outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring';

export function ContactEditor({
  mode,
  createTargets,
  editableCards,
  writeStates,
  onClose,
}: ContactEditorProps) {
  const defaultBook =
    createTargets.find((target) => target.is_default) ?? createTargets[0];
  const [bookKey, setBookKey] = useState(
    defaultBook !== undefined ? `${defaultBook.account}:${defaultBook.book}` : '',
  );
  const [chosenCard, setChosenCard] = useState<EditableCardRow | null>(
    editableCards.length === 1 ? editableCards[0]! : null,
  );
  const [pickedKey, setPickedKey] = useState('');
  const [form, setForm] = useState<EditorForm>(EMPTY_FORM);
  const [baseline, setBaseline] = useState<EditorForm>(EMPTY_FORM);
  const [watch, setWatch] = useState<{ account: string; baseline: PeopleWriteRow[] } | null>(
    null,
  );

  useEffect(() => {
    if (mode !== 'edit' || chosenCard === null) {
      return;
    }
    let standing = true;
    peopleCard(chosenCard)
      .then((seed) => {
        if (standing) {
          const rendered = formOfSeed(seed);
          setForm(rendered);
          setBaseline(rendered);
        }
      })
      .catch(() => {
        // A seed that cannot be read leaves the form empty; the save stays
        // off until something is typed.
      });
    return () => {
      standing = false;
    };
  }, [mode, chosenCard]);

  const chosenTarget = createTargets.find(
    (target) => `${target.account}:${target.book}` === bookKey,
  );
  const choosingCard = mode === 'edit' && editableCards.length > 1 && chosenCard === null;
  const savedAccount = mode === 'edit' ? chosenCard?.account : chosenTarget?.account;
  const invalid =
    savedAccount !== undefined &&
    writeStates.some((row) => row.account === savedAccount && row.state === 'invalid');
  const changed =
    mode === 'edit' &&
    (form.given !== baseline.given ||
      form.surname !== baseline.surname ||
      form.organisation !== baseline.organisation ||
      form.role !== baseline.role ||
      form.emails !== baseline.emails ||
      form.phones !== baseline.phones);
  // The host's findable-again rule, mirrored as the affordance: a create
  // that names nothing (no name part, no organisation, no address) has
  // nothing the host would file it under, so Save stays off.
  const findable =
    form.given.trim() !== '' ||
    form.surname.trim() !== '' ||
    form.organisation.trim() !== '' ||
    listOf(form.emails).length > 0;

  // The close waits for the write the dispatch started: the write states
  // arrive through the store subscription, so the watch holds the account
  // written to and the rows last seen, and decides only on a publish that
  // follows the save.
  useEffect(() => {
    if (watch === null || writeStates === watch.baseline) {
      return;
    }
    const row = writeStates.find((candidate) => candidate.account === watch.account);
    if (row?.state === 'invalid') {
      // The refusal sentence stands under the form the user is still
      // looking at; only a later write can clear it.
      setWatch(null);
      return;
    }
    if (row?.state === 'writing') {
      // The publish caught the write in flight; its settle publishes again.
      setWatch({ account: watch.account, baseline: writeStates });
      return;
    }
    // Absent (settled clean) or failed-with-retry (the write landed; the
    // banner beside the list owns its recovery): the form's work is done.
    setWatch(null);
    onClose();
  }, [writeStates, watch, onClose]);

  const set = (field: keyof EditorForm, value: string): void =>
    setForm((current) => ({ ...current, [field]: value }));

  const save = (): void => {
    if (mode === 'create') {
      if (chosenTarget === undefined) {
        return;
      }
      const name = nameOf(form);
      const draft: ContactDraftInput = {
        ...(name !== undefined ? { name } : {}),
        ...(form.organisation.trim() !== '' ? { organisation: form.organisation.trim() } : {}),
        ...(form.role.trim() !== '' ? { role: form.role.trim() } : {}),
        ...(listOf(form.emails).length > 0 ? { emails: listOf(form.emails) } : {}),
        ...(listOf(form.phones).length > 0 ? { phones: listOf(form.phones) } : {}),
      };
      void dispatch(intents.createContact({ account: chosenTarget.account, book: chosenTarget.book }, draft));
      setWatch({ account: chosenTarget.account, baseline: writeStates });
      return;
    }
    if (chosenCard === null) {
      return;
    }
    const patch: ContactPatchInput = {};
    if (form.given !== baseline.given || form.surname !== baseline.surname) {
      const name = nameOf(form);
      if (name !== undefined) {
        patch.name = name;
      }
    }
    if (form.organisation !== baseline.organisation) {
      patch.organisation = form.organisation.trim();
    }
    if (form.role !== baseline.role) {
      patch.role = form.role.trim();
    }
    if (listOf(form.emails).join('\u{0}') !== listOf(baseline.emails).join('\u{0}')) {
      patch.emails = listOf(form.emails);
    }
    if (listOf(form.phones).join('\u{0}') !== listOf(baseline.phones).join('\u{0}')) {
      patch.phones = listOf(form.phones);
    }
    if (Object.keys(patch).length > 0) {
      void dispatch(intents.updateContact(chosenCard, patch));
      setWatch({ account: chosenCard.account, baseline: writeStates });
      return;
    }
    onClose();
  };

  return (
    <div className="fixed inset-0 z-[var(--z-modal)] flex items-center justify-center bg-[var(--backdrop)] p-4">
      <div
        role="dialog"
        aria-label={mode === 'create' ? 'New contact' : 'Edit contact'}
        className="flex max-h-full w-[420px] flex-col overflow-y-auto rounded-[var(--radius)] border border-border bg-surface-floating p-4 shadow-[var(--shadow-lg)]"
      >
        <h2 className="mb-3 text-sm font-semibold text-foreground">
          {mode === 'create' ? 'New contact' : 'Edit contact'}
        </h2>

        {choosingCard ? (
          <fieldset className="flex flex-col gap-2">
            <legend className="mb-1 text-xs text-muted-foreground">
              This person has more than one card. Choose the one to edit.
            </legend>
            {editableCards.map((card) => (
              <label key={`${card.account}:${card.card}`} className="flex items-center gap-2 text-xs text-foreground">
                <input
                  type="radio"
                  name="edit-card"
                  aria-label={`Edit in ${card.account}`}
                  checked={pickedKey === `${card.account}:${card.card}`}
                  onChange={() => setPickedKey(`${card.account}:${card.card}`)}
                />
                {card.account}
              </label>
            ))}
            <div className="mt-3 flex justify-end gap-2 border-t border-border pt-3">
              <button
                type="button"
                onClick={onClose}
                className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground"
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={pickedKey === ''}
                onClick={() => {
                  const card = editableCards.find(
                    (candidate) => `${candidate.account}:${candidate.card}` === pickedKey,
                  );
                  if (card !== undefined) {
                    setChosenCard(card);
                  }
                }}
                className="rounded-[var(--radius)] bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground transition-colors hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
              >
                Continue
              </button>
            </div>
          </fieldset>
        ) : (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              save();
            }}
            className="flex flex-col gap-3"
          >
            {mode === 'create' && (
              <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                Address book
                <select
                  aria-label="Address book"
                  value={bookKey}
                  onChange={(event) => setBookKey(event.target.value)}
                  className={fieldClass}
                >
                  {createTargets.map((target) => (
                    <option
                      key={`${target.account}:${target.book}`}
                      value={`${target.account}:${target.book}`}
                    >
                      {target.name}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <div className="flex gap-2">
              <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
                Given name
                <input
                  aria-label="Given name"
                  value={form.given}
                  onChange={(event) => set('given', event.target.value)}
                  className={fieldClass}
                />
              </label>
              <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
                Surname
                <input
                  aria-label="Surname"
                  value={form.surname}
                  onChange={(event) => set('surname', event.target.value)}
                  className={fieldClass}
                />
              </label>
            </div>
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Organisation
              <input
                aria-label="Organisation"
                value={form.organisation}
                onChange={(event) => set('organisation', event.target.value)}
                className={fieldClass}
              />
            </label>
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Role
              <input
                aria-label="Role"
                value={form.role}
                onChange={(event) => set('role', event.target.value)}
                className={fieldClass}
              />
            </label>
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Emails
              <input
                aria-label="Emails"
                value={form.emails}
                onChange={(event) => set('emails', event.target.value)}
                placeholder="Comma-separated"
                className={fieldClass}
              />
            </label>
            <label className="flex flex-col gap-1 text-xs text-muted-foreground">
              Phones
              <input
                aria-label="Phones"
                value={form.phones}
                onChange={(event) => set('phones', event.target.value)}
                placeholder="Comma-separated"
                className={fieldClass}
              />
            </label>

            {invalid && (
              <p role="alert" className="text-xs text-[var(--warning)]">
                {INVALID_SENTENCE}
              </p>
            )}

            <div className="flex justify-end gap-2 border-t border-border pt-3">
              <button
                type="button"
                onClick={onClose}
                className="rounded-[var(--radius)] border border-border px-2.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-hover hover:text-foreground"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={mode === 'edit' ? !changed : !findable}
                className="rounded-[var(--radius)] bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground transition-colors hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
              >
                Save
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}

/** The name the form states: present when either part carries words, the
 * surname keyed only when it carries some. */
function nameOf(form: EditorForm): { given: string; surname?: string } | undefined {
  if (form.given.trim() === '' && form.surname.trim() === '') {
    return undefined;
  }
  return {
    given: form.given.trim(),
    ...(form.surname.trim() !== '' ? { surname: form.surname.trim() } : {}),
  };
}
